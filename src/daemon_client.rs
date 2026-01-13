//! Client for communicating with the vibed daemon

use anyhow::{Context, Result};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::daemon_ipc::{get_socket_path, DaemonRequest, DaemonResponse};
use crate::VERSION;

/// Client for communicating with the vibed daemon
pub struct DaemonClient {
    stream: UnixStream,
}

impl DaemonClient {
    /// Connect to the daemon for a repository
    pub async fn connect(repo_path: &Path) -> Result<Self> {
        let socket_path = get_socket_path(repo_path);

        let stream = UnixStream::connect(&socket_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to daemon at {}. Is vibed running?",
                    socket_path.display()
                )
            })?;

        Ok(Self { stream })
    }

    /// Connect to the daemon and verify version matches
    pub async fn connect_with_version_check(repo_path: &Path) -> Result<Self> {
        let mut client = Self::connect(repo_path).await?;

        // Ping to get version
        match client.request(DaemonRequest::Ping).await? {
            DaemonResponse::Pong { version } => {
                if let Some(daemon_version) = version {
                    if daemon_version != VERSION {
                        anyhow::bail!(
                            "Version mismatch: vibe CLI is v{} but daemon is v{}.\n\
                             Run 'vibe daemon stop' and retry to start a new daemon.",
                            VERSION,
                            daemon_version
                        );
                    }
                }
                // No version in response means old daemon - proceed with warning
            }
            _ => {}
        }

        Ok(client)
    }

    /// Check if daemon is running for a repository
    pub async fn is_running(repo_path: &Path) -> bool {
        Self::connect(repo_path).await.is_ok()
    }

    /// Send a request and receive a response
    async fn request(&mut self, req: DaemonRequest) -> Result<DaemonResponse> {
        let json = serde_json::to_string(&req)? + "\n";
        self.stream.write_all(json.as_bytes()).await?;

        let mut reader = BufReader::new(&mut self.stream);
        let mut response = String::new();
        reader.read_line(&mut response).await?;

        let resp: DaemonResponse = serde_json::from_str(response.trim())?;
        Ok(resp)
    }

    /// Ping the daemon
    pub async fn ping(&mut self) -> Result<bool> {
        match self.request(DaemonRequest::Ping).await? {
            DaemonResponse::Pong { .. } => Ok(true),
            _ => Ok(false),
        }
    }

    /// Get daemon status
    pub async fn status(&mut self) -> Result<DaemonResponse> {
        self.request(DaemonRequest::Status).await
    }

    /// Export a session (create/mount)
    pub async fn export_session(&mut self, vibe_id: &str) -> Result<DaemonResponse> {
        self.request(DaemonRequest::ExportSession {
            vibe_id: vibe_id.to_string(),
        })
        .await
    }

    /// Unexport a session (unmount/cleanup)
    pub async fn unexport_session(&mut self, vibe_id: &str) -> Result<DaemonResponse> {
        self.request(DaemonRequest::UnexportSession {
            vibe_id: vibe_id.to_string(),
        })
        .await
    }

    /// List active sessions
    pub async fn list_sessions(&mut self) -> Result<DaemonResponse> {
        self.request(DaemonRequest::ListSessions).await
    }

    /// Request daemon shutdown
    pub async fn shutdown(&mut self) -> Result<DaemonResponse> {
        self.request(DaemonRequest::Shutdown).await
    }
}

/// Start the daemon if not running, with version check
pub async fn ensure_daemon_running(repo_path: &Path) -> Result<()> {
    // First check if a daemon is already running
    if let Ok(mut client) = DaemonClient::connect(repo_path).await {
        // Check version
        if let Ok(DaemonResponse::Pong { version }) = client.request(DaemonRequest::Ping).await {
            if let Some(daemon_version) = version {
                if daemon_version != VERSION {
                    eprintln!(
                        "Warning: Running daemon is v{} but CLI is v{}. Stopping old daemon...",
                        daemon_version, VERSION
                    );
                    // Stop the old daemon
                    let _ = client.shutdown().await;
                    // Give it time to shut down
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                } else {
                    // Version matches, we're good
                    return Ok(());
                }
            } else {
                // Old daemon without version - stop it
                eprintln!("Warning: Running daemon is outdated (no version). Stopping...");
                let _ = client.shutdown().await;
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        } else {
            // Daemon is running and responded
            return Ok(());
        }
    }

    // Start the daemon
    let vibed_path = std::env::current_exe()?
        .parent()
        .unwrap()
        .join("vibed");

    // If vibed is not in the same directory, try PATH
    let vibed_cmd = if vibed_path.exists() {
        vibed_path.to_string_lossy().to_string()
    } else {
        "vibed".to_string()
    };

    let repo_path_str = repo_path.to_string_lossy();
    let log_path = repo_path.join(".vibe").join("vibed.log");

    // Clear old log file to get fresh error output
    let _ = std::fs::remove_file(&log_path);

    std::process::Command::new(&vibed_cmd)
        .args(["-r", &repo_path_str])
        .spawn()
        .with_context(|| format!("Failed to start daemon: {}", vibed_cmd))?;

    // Wait for daemon to be ready
    for _ in 0..50 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if DaemonClient::is_running(repo_path).await {
            return Ok(());
        }
    }

    // Daemon failed to start - try to get error from log file
    let error_detail = if log_path.exists() {
        match std::fs::read_to_string(&log_path) {
            Ok(log) => {
                let lines: Vec<&str> = log.lines().collect();
                let last_lines: Vec<&str> = lines.iter().rev().take(10).copied().collect();
                if last_lines.is_empty() {
                    String::new()
                } else {
                    format!("\n\nDaemon log ({}):\n{}", log_path.display(), last_lines.into_iter().rev().collect::<Vec<_>>().join("\n"))
                }
            }
            Err(_) => String::new(),
        }
    } else {
        String::new()
    };

    anyhow::bail!("Daemon failed to start within 5 seconds.{}", error_detail)
}

/// Start the daemon in foreground mode (for debugging)
pub async fn start_daemon_foreground(repo_path: &Path) -> Result<()> {
    let vibed_path = std::env::current_exe()?
        .parent()
        .unwrap()
        .join("vibed");

    let vibed_cmd = if vibed_path.exists() {
        vibed_path.to_string_lossy().to_string()
    } else {
        "vibed".to_string()
    };

    let repo_path_str = repo_path.to_string_lossy();

    let status = std::process::Command::new(&vibed_cmd)
        .args(["-r", &repo_path_str, "-f"])
        .status()
        .with_context(|| format!("Failed to start daemon: {}", vibed_cmd))?;

    if !status.success() {
        anyhow::bail!("Daemon exited with status: {}", status);
    }

    Ok(())
}
