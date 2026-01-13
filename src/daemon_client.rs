//! Client for communicating with the vibed daemon

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::daemon_ipc::{get_socket_path, DaemonRequest, DaemonResponse};
use crate::VERSION;

/// Clean up stale daemon state (socket, PID file, log) if daemon is not running
async fn cleanup_stale_daemon_state(socket_path: &PathBuf, pid_path: &PathBuf, log_path: &PathBuf) {
    let mut cleaned = false;

    // Check if PID file exists and process is dead
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                // Check if process is still running (Unix-specific)
                #[cfg(unix)]
                {
                    // kill -0 checks if process exists without sending a signal
                    let exists = unsafe { libc::kill(pid, 0) == 0 };
                    if !exists {
                        eprintln!("  Cleaning up stale PID file (process {} not running)", pid);
                        let _ = std::fs::remove_file(pid_path);
                        cleaned = true;
                    }
                }
            }
        }
    }

    // Check if socket exists but daemon isn't responding
    if socket_path.exists() {
        if tokio::net::UnixStream::connect(socket_path).await.is_err() {
            eprintln!("  Cleaning up stale socket file");
            let _ = std::fs::remove_file(socket_path);
            cleaned = true;
        }
    }

    // Clear old log file for fresh diagnostics
    if cleaned || log_path.exists() {
        let _ = std::fs::remove_file(log_path);
    }
}

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
    let socket_path = crate::daemon_ipc::get_socket_path(repo_path);
    let pid_path = crate::daemon_ipc::get_pid_path(repo_path);

    // Clean up stale state from crashed daemon
    cleanup_stale_daemon_state(&socket_path, &pid_path, &log_path).await;

    eprintln!("  Starting daemon: {}", vibed_cmd);

    let mut child = std::process::Command::new(&vibed_cmd)
        .args(["-r", &repo_path_str])
        .spawn()
        .with_context(|| format!("Failed to start daemon: {}", vibed_cmd))?;

    // Wait for daemon to be ready, checking if process died
    for i in 0..50 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check if child process died (only works for direct child, not daemonized)
        if i < 5 {
            // Only check early - after daemonizing, the child exits normally
            if let Ok(Some(status)) = child.try_wait() {
                if !status.success() {
                    let code = status.code().unwrap_or(-1);
                    anyhow::bail!(
                        "Daemon process exited immediately with code {}.\n\
                         Binary: {}\n\
                         This may indicate the binary was killed by macOS security.\n\
                         Try running 'vibed -f' manually to see the error.",
                        code,
                        vibed_cmd
                    );
                }
            }
        }

        if DaemonClient::is_running(repo_path).await {
            return Ok(());
        }
    }

    // Daemon failed to start - gather diagnostics
    let mut error_detail = format!("\n\nBinary used: {}", vibed_cmd);

    // Check if log file has any content
    if log_path.exists() {
        if let Ok(log) = std::fs::read_to_string(&log_path) {
            let lines: Vec<&str> = log.lines().collect();
            if !lines.is_empty() {
                let last_lines: Vec<&str> = lines.iter().rev().take(10).copied().collect();
                error_detail.push_str(&format!(
                    "\n\nDaemon log ({}):\n{}",
                    log_path.display(),
                    last_lines.into_iter().rev().collect::<Vec<_>>().join("\n")
                ));
            } else {
                error_detail.push_str("\n\nDaemon log exists but is empty.");
            }
        }
    } else {
        error_detail.push_str(&format!(
            "\n\nNo daemon log found at {}.\n\
             The daemon may have been killed before it could start.\n\
             Try running 'vibed -f -r {}' manually to see the error.",
            log_path.display(),
            repo_path_str
        ));
    }

    // Check if socket was created but daemon isn't responding
    if socket_path.exists() {
        error_detail.push_str("\n\nSocket file exists but daemon is not responding. Possible stale socket.");
    }

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
