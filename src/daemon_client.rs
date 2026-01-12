//! Client for communicating with the vibed daemon

use anyhow::{Context, Result};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::daemon_ipc::{get_socket_path, DaemonRequest, DaemonResponse};

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
            DaemonResponse::Pong => Ok(true),
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

/// Start the daemon if not running
pub async fn ensure_daemon_running(repo_path: &Path) -> Result<()> {
    if DaemonClient::is_running(repo_path).await {
        return Ok(());
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

    anyhow::bail!("Daemon failed to start within 5 seconds")
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
