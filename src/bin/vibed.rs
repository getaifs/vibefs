//! vibed - VibeFS Background Daemon
//!
//! The ephemeral daemon that serves the NFSv4 virtual filesystem.
//! It manages sessions, handles NFS requests, and auto-shutdowns after idleness.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{Mutex, RwLock};

use vibefs::db::MetadataStore;
use vibefs::git::GitRepo;

/// Default idle timeout: 20 minutes
const IDLE_TIMEOUT_SECS: u64 = 20 * 60;

/// Session state managed by the daemon
#[allow(dead_code)]
struct Session {
    vibe_id: String,
    session_dir: PathBuf,
    mount_point: PathBuf,
    nfs_port: u16,
    created_at: Instant,
}

/// Daemon state shared across handlers
struct DaemonState {
    repo_path: PathBuf,
    #[allow(dead_code)]
    metadata: Arc<RwLock<MetadataStore>>,
    #[allow(dead_code)]
    git: Arc<RwLock<GitRepo>>,
    sessions: HashMap<String, Session>,
    last_activity: Instant,
    nfs_listener: Option<std::net::TcpListener>,
    nfs_port: u16,
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
    Shutdown,
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
        sessions: Vec<SessionInfo>,
    },
    ShuttingDown,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionInfo {
    vibe_id: String,
    mount_point: String,
    nfs_port: u16,
    uptime_secs: u64,
}

/// Get the Unix Domain Socket path for a repository
fn get_socket_path(repo_path: &Path) -> PathBuf {
    let vibe_dir = repo_path.join(".vibe");
    vibe_dir.join("vibed.sock")
}

/// Get the PID file path
fn get_pid_path(repo_path: &Path) -> PathBuf {
    let vibe_dir = repo_path.join(".vibe");
    vibe_dir.join("vibed.pid")
}

/// Find an available high port for NFS
fn find_available_port() -> Result<(std::net::TcpListener, u16)> {
    // Bind to port 0 to let OS assign a free high port
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .context("Failed to bind to any port")?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

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
                    nfs_port: state.nfs_port,
                    session_count: state.sessions.len(),
                    uptime_secs: start_time.elapsed().as_secs(),
                }
            }

            DaemonRequest::ExportSession { vibe_id } => {
                let mut state = state.lock().await;

                // Check if session already exists
                if let Some(session) = state.sessions.get(&vibe_id) {
                    DaemonResponse::SessionExported {
                        vibe_id: session.vibe_id.clone(),
                        nfs_port: session.nfs_port,
                        mount_point: session.mount_point.display().to_string(),
                    }
                } else {
                    // Create new session
                    let session_dir = state.repo_path.join(".vibe/sessions").join(&vibe_id);
                    let mount_point = PathBuf::from(format!(
                        "{}/Library/Caches/vibe/mounts/{}",
                        std::env::var("HOME").unwrap_or_default(),
                        vibe_id
                    ));

                    // Create directories
                    if let Err(e) = std::fs::create_dir_all(&session_dir) {
                        DaemonResponse::Error {
                            message: format!("Failed to create session dir: {}", e),
                        }
                    } else if let Err(e) = std::fs::create_dir_all(&mount_point) {
                        DaemonResponse::Error {
                            message: format!("Failed to create mount point: {}", e),
                        }
                    } else {
                        let session = Session {
                            vibe_id: vibe_id.clone(),
                            session_dir,
                            mount_point: mount_point.clone(),
                            nfs_port: state.nfs_port,
                            created_at: Instant::now(),
                        };

                        state.sessions.insert(vibe_id.clone(), session);

                        DaemonResponse::SessionExported {
                            vibe_id,
                            nfs_port: state.nfs_port,
                            mount_point: mount_point.display().to_string(),
                        }
                    }
                }
            }

            DaemonRequest::UnexportSession { vibe_id } => {
                let mut state = state.lock().await;
                if state.sessions.remove(&vibe_id).is_some() {
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

/// Run the NFS server (placeholder - actual NFS implementation in separate module)
async fn run_nfs_server(
    _listener: std::net::TcpListener,
    _state: Arc<Mutex<DaemonState>>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    // Note: In the full implementation, this would use nfsserve to handle NFS requests.
    // For now, we keep the listener active so the port stays reserved.

    eprintln!("[vibed] NFS server placeholder running (actual NFS not yet integrated)");

    // Wait for shutdown signal
    let _ = shutdown_rx.recv().await;

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

    // Find available port for NFS
    let (nfs_listener, nfs_port) = find_available_port()?;

    eprintln!(
        "[vibed] Starting daemon for {} (NFS port: {})",
        repo_path.display(),
        nfs_port
    );

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
        nfs_listener: Some(nfs_listener),
        nfs_port,
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

    // Start NFS server task
    let nfs_state = state.clone();
    let nfs_shutdown_rx = shutdown_tx.subscribe();
    let nfs_listener = {
        let mut s = state.lock().await;
        s.nfs_listener.take().unwrap()
    };
    let nfs_handle = tokio::spawn(async move {
        if let Err(e) = run_nfs_server(nfs_listener, nfs_state, nfs_shutdown_rx).await {
            eprintln!("[vibed] NFS server error: {}", e);
        }
    });

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

    // Wait for tasks to finish
    nfs_handle.abort();
    idle_handle.abort();

    if !foreground {
        eprintln!("[vibed] Daemon stopped");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
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
        run_daemon(repo_path, true).await
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
                // Re-initialize tokio runtime since fork() invalidates it
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()?
                    .block_on(run_daemon(repo_path, false))
            }
            Err(e) => anyhow::bail!("Failed to daemonize: {}", e),
        }
    }
}
