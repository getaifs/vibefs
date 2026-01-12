//! vibed - VibeFS Background Daemon
//! 
//! The ephemeral daemon that serves the NFSv4 virtual filesystem.
//! It manages sessions, handles NFS requests, and auto-shutdowns after idleness.

use anyhow::{Context, Result};
use nfsserve::tcp::{NFSTcp, NFSTcpListener};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{Mutex, RwLock};

use vibefs::db::MetadataStore;
use vibefs::git::GitRepo;
use vibefs::nfs::VibeNFS;

/// Default idle timeout: 20 minutes
const IDLE_TIMEOUT_SECS: u64 = 20 * 60;

/// Session state managed by the daemon
struct Session {
    vibe_id: String,
    #[allow(dead_code)]
    session_dir: PathBuf,
    mount_point: PathBuf,
    nfs_port: u16,
    created_at: Instant,
    shutdown_tx: tokio::sync::broadcast::Sender<()>
}

/// Daemon state shared across handlers
struct DaemonState {
    repo_path: PathBuf,
    metadata: Arc<RwLock<MetadataStore>>,
    git: Arc<RwLock<GitRepo>>,
    sessions: HashMap<String, Session>,
    last_activity: Instant
}

impl DaemonState {
    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    fn is_idle(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

/// IPC message types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
enum DaemonRequest {
    /// Ping to check daemon is alive
    Ping,
    /// Get daemon status
    Status,
    /// Create/export a new session
    ExportSession { vibe_id: String },
    /// Unexport/remove a session
    UnexportSession { vibe_id: String },
    /// List active sessions
    ListSessions,
    /// Graceful shutdown
    Shutdown
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
enum DaemonResponse {
    Pong,
    Status {
        repo_path: String,
        nfs_port: u16,
        session_count: usize,
        uptime_secs: u64,
    },
    SessionExported {
        vibe_id: String,
        nfs_port: u16,
        mount_point: String,
    },
    SessionUnexported {
        vibe_id: String,
    },
    Sessions {
        sessions: Vec<SessionInfo>
    },
    ShuttingDown,
    Error {
        message: String
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionInfo {
    vibe_id: String,
    mount_point: String,
    nfs_port: u16,
    uptime_secs: u64
}

/// Get the Unix Domain Socket path for a repository
fn get_socket_path(repo_path: &Path) -> PathBuf {
    let vibe_dir = repo_path.join(".vibe");
    vibe_dir.join("vibed.sock")
}

use vibefs::daemon_ipc::get_pid_path;

/// Handle a single client connection
async fn handle_client(
    stream: tokio::net::UnixStream,
    state: Arc<Mutex<DaemonState>>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    start_time: Instant,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let request: DaemonRequest = match serde_json::from_str(line.trim()) {
            Ok(req) => req,
            Err(e) => {
                let response = DaemonResponse::Error {
                    message: format!("Invalid request: {}", e),
                };
                let json = serde_json::to_string(&response)? + "\n";
                writer.write_all(json.as_bytes()).await?;
                line.clear();
                continue;
            }
        };

        // Update last activity
        {
            let mut state = state.lock().await;
            state.touch();
        }

        let response = match request {
            DaemonRequest::Ping => DaemonResponse::Pong,

            DaemonRequest::Status => {
                let state = state.lock().await;
                DaemonResponse::Status {
                    repo_path: state.repo_path.display().to_string(),
                    nfs_port: 0, // Using per-session ports now
                    session_count: state.sessions.len(),
                    uptime_secs: start_time.elapsed().as_secs(),
                }
            }

            DaemonRequest::ExportSession { vibe_id } => {
                let mut state_guard = state.lock().await;

                // Check if session already exists
                if let Some(session) = state_guard.sessions.get(&vibe_id) {
                    DaemonResponse::SessionExported {
                        vibe_id: session.vibe_id.clone(),
                        nfs_port: session.nfs_port,
                        mount_point: session.mount_point.display().to_string(),
                    }
                } else {
                    // Create new session
                    let session_dir = state_guard.repo_path.join(".vibe/sessions").join(&vibe_id);

                    // Get repo name for mount point
                    let repo_name = state_guard.repo_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "repo".to_string());

                    // Mount point format: ~/Library/Caches/vibe/mounts/<repo_name>-<vibe_id>
                    let mount_point = PathBuf::from(format!(
                        "{}/Library/Caches/vibe/mounts/{}-{}",
                        std::env::var("HOME").unwrap_or_default(),
                        repo_name,
                        vibe_id
                    ));

                    match setup_session_resources(&session_dir, &mount_point) {
                        Ok(_) => {
                            let nfs = VibeNFS::new(
                                state_guard.metadata.clone(), 
                                state_guard.git.clone(), 
                                session_dir.clone(), 
                                vibe_id.clone()
                            );
                            
                            if let Err(e) = nfs.build_directory_cache().await {
                                DaemonResponse::Error {
                                    message: format!("Failed to build cache: {}", e),
                                }
                            } else {
                                // Bind NFS listener
                                match NFSTcpListener::bind("127.0.0.1:0", nfs).await {
                                    Ok(listener) => {
                                        let port = listener.get_listen_port();
                                        let (sess_shutdown_tx, mut sess_shutdown_rx) = tokio::sync::broadcast::channel(1);
                                        let vid = vibe_id.clone();
                                        
                                        // Spawn NFS server task
                                        tokio::spawn(async move {
                                            eprintln!("[vibed] NFS server running for {} on port {}", vid, port);
                                            tokio::select! {
                                                res = listener.handle_forever() => {
                                                    if let Err(e) = res {
                                                        eprintln!("[vibed] NFS server error for {}: {}", vid, e);
                                                    }
                                                }
                                                _ = sess_shutdown_rx.recv() => {
                                                    eprintln!("[vibed] Stopping NFS server for {}", vid);
                                                }
                                            }
                                        });

                                        let session = Session {
                                            vibe_id: vibe_id.clone(),
                                            session_dir,
                                            mount_point: mount_point.clone(),
                                            nfs_port: port,
                                            created_at: Instant::now(),
                                            shutdown_tx: sess_shutdown_tx,
                                        };

                                        state_guard.sessions.insert(vibe_id.clone(), session);

                                        DaemonResponse::SessionExported {
                                            vibe_id,
                                            nfs_port: port,
                                            mount_point: mount_point.display().to_string(),
                                        }
                                    }
                                    Err(e) => DaemonResponse::Error {
                                        message: format!("Failed to bind NFS port: {}", e),
                                    }
                                }
                            }
                        }
                        Err(e) => DaemonResponse::Error {
                            message: format!("Failed to create directories: {}", e),
                        }
                    }
                }
            }

            DaemonRequest::UnexportSession { vibe_id } => {
                let mut state = state.lock().await;
                if let Some(session) = state.sessions.remove(&vibe_id) {
                    // Stop the NFS server for this session
                    let _ = session.shutdown_tx.send(());
                    DaemonResponse::SessionUnexported { vibe_id }
                } else {
                    DaemonResponse::Error {
                        message: format!("Session '{}' not found", vibe_id),
                    }
                }
            }

            DaemonRequest::ListSessions => {
                let state = state.lock().await;
                let sessions: Vec<SessionInfo> = state
                    .sessions
                    .values()
                    .map(|s| SessionInfo {
                        vibe_id: s.vibe_id.clone(),
                        mount_point: s.mount_point.display().to_string(),
                        nfs_port: s.nfs_port,
                        uptime_secs: s.created_at.elapsed().as_secs(),
                    })
                    .collect();

                DaemonResponse::Sessions { sessions }
            }

            DaemonRequest::Shutdown => {
                let _ = shutdown_tx.send(());
                DaemonResponse::ShuttingDown
            }
        };

        let json = serde_json::to_string(&response)? + "\n";
        writer.write_all(json.as_bytes()).await?;
        line.clear();
    }

    Ok(())
}

fn setup_session_resources(session_dir: &Path, mount_point: &Path) -> Result<()> {
    std::fs::create_dir_all(session_dir)?;
    std::fs::create_dir_all(mount_point)?;
    Ok(())
}

/// Run the idle timeout checker
async fn run_idle_checker(
    state: Arc<Mutex<DaemonState>>,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    timeout: Duration,
) {
    let check_interval = Duration::from_secs(60);

    loop {
        tokio::time::sleep(check_interval).await;

        let is_idle = {
            let state = state.lock().await;
            state.is_idle(timeout) && state.sessions.is_empty()
        };

        if is_idle {
            eprintln!(
                "[vibed] Idle timeout reached ({} minutes), shutting down",
                timeout.as_secs() / 60
            );
            let _ = shutdown_tx.send(());
            break;
        }
    }
}

/// Main daemon entry point
async fn run_daemon(repo_path: PathBuf, foreground: bool) -> Result<()> {
    let vibe_dir = repo_path.join(".vibe");

    // Verify VibeFS is initialized
    if !vibe_dir.exists() {
        anyhow::bail!(
            "VibeFS not initialized at {}. Run 'vibe init' first.",
            repo_path.display()
        );
    }

    let socket_path = get_socket_path(&repo_path);
    let pid_path = get_pid_path(&repo_path);

    // Check if daemon is already running
    if socket_path.exists() {
        // Try to connect to see if it's alive
        if tokio::net::UnixStream::connect(&socket_path).await.is_ok() {
            anyhow::bail!("Daemon already running for this repository");
        }
        // Stale socket, remove it
        std::fs::remove_file(&socket_path).ok();
    }

    eprintln!("[vibed] Starting daemon for {}", repo_path.display());

    // Open metadata and git
    let metadata = MetadataStore::open(vibe_dir.join("metadata.db"))
        .context("Failed to open metadata store")?;
    let git = GitRepo::open(&repo_path).context("Failed to open Git repository")?;

    // Create daemon state
    let state = Arc::new(Mutex::new(DaemonState {
        repo_path: repo_path.clone(),
        metadata: Arc::new(RwLock::new(metadata)),
        git: Arc::new(RwLock::new(git)),
        sessions: HashMap::new(),
        last_activity: Instant::now(),
    }));

    // Write PID file
    std::fs::write(&pid_path, std::process::id().to_string())?;

    // Create Unix socket listener
    let uds_listener =
        UnixListener::bind(&socket_path).context("Failed to bind Unix domain socket")?;

    eprintln!("[vibed] Listening on {}", socket_path.display());

    // Shutdown channel
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    let start_time = Instant::now();

    // Start idle checker task
    let idle_state = state.clone();
    let idle_shutdown_tx = shutdown_tx.clone();
    let idle_timeout = Duration::from_secs(IDLE_TIMEOUT_SECS);
    let idle_handle = tokio::spawn(async move {
        run_idle_checker(idle_state, idle_shutdown_tx, idle_timeout).await;
    });

    // Accept client connections
    let mut shutdown_rx = shutdown_tx.subscribe();

    loop {
        tokio::select! {
            result = uds_listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let client_state = state.clone();
                        let client_shutdown_tx = shutdown_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_client(stream, client_state, client_shutdown_tx, start_time).await {
                                eprintln!("[vibed] Client handler error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("[vibed] Accept error: {}", e);
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                eprintln!("[vibed] Shutdown signal received");
                break;
            }
        }
    }

    // Cleanup
    eprintln!("[vibed] Cleaning up...");
    std::fs::remove_file(&socket_path).ok();
    std::fs::remove_file(&pid_path).ok();
    
    // Stop all sessions
    {
        let mut s = state.lock().await;
        for (_, session) in s.sessions.drain() {
            let _ = session.shutdown_tx.send(());
        }
    }

    // Wait for tasks to finish (idle checker)
    idle_handle.abort();

    if !foreground {
        eprintln!("[vibed] Daemon stopped");
    }

    Ok(())
}

fn main() -> Result<()> {
    use clap::{Arg, Command};

    let matches = Command::new("vibed")
        .about("VibeFS Background Daemon")
        .arg(
            Arg::new("repo")
                .short('r')
                .long("repo")
                .value_name("PATH")
                .help("Path to the Git repository")
                .default_value("."),
        )
        .arg(
            Arg::new("foreground")
                .short('f')
                .long("foreground")
                .help("Run in foreground (don't daemonize)")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let repo_path = PathBuf::from(matches.get_one::<String>("repo").unwrap());
    let repo_path = repo_path
        .canonicalize()
        .context("Failed to resolve repository path")?;

    let foreground = matches.get_flag("foreground");

    if foreground {
        // Run directly in foreground
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()? 
            .block_on(run_daemon(repo_path, true))
    } else {
        // Daemonize
        use daemonize::Daemonize;

        let vibe_dir = repo_path.join(".vibe");
        let stdout = std::fs::File::create(vibe_dir.join("vibed.log"))
            .context("Failed to create log file")?;
        let stderr = stdout.try_clone()?;

        let daemonize = Daemonize::new()
            .working_directory(&repo_path)
            .stdout(stdout)
            .stderr(stderr);

        match daemonize.start() {
            Ok(_) => {
                // We're now in the daemon process
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()?
                    .block_on(run_daemon(repo_path, false))
            }
            Err(e) => anyhow::bail!("Failed to daemonize: {}", e),
        }
    }
}