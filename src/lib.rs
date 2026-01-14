pub mod db;
pub mod git;
pub mod gitignore;
pub mod names;
pub mod nfs;
pub mod tui;
pub mod commands;
pub mod cwd_validation;
pub mod daemon_client;
pub mod platform;

/// Package version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// IPC message types for daemon communication
pub mod daemon_ipc {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum DaemonRequest {
        Ping,
        Status,
        ExportSession { vibe_id: String },
        UnexportSession { vibe_id: String },
        ListSessions,
        Shutdown,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum DaemonResponse {
        Pong {
            #[serde(default)]
            version: Option<String>,
        },
        Status {
            repo_path: String,
            nfs_port: u16,
            session_count: usize,
            uptime_secs: u64,
            #[serde(default)]
            version: Option<String>,
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

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SessionInfo {
        pub vibe_id: String,
        pub mount_point: String,
        pub nfs_port: u16,
        pub uptime_secs: u64,
    }

    /// Get the Unix Domain Socket path for a repository
    pub fn get_socket_path(repo_path: &std::path::Path) -> std::path::PathBuf {
        repo_path.join(".vibe").join("vibed.sock")
    }

    /// Get the PID file path for a repository
    pub fn get_pid_path(repo_path: &std::path::Path) -> std::path::PathBuf {
        repo_path.join(".vibe").join("vibed.pid")
    }
}
