use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use vibefs::commands;
use vibefs::daemon_client::{self, DaemonClient};
use vibefs::daemon_ipc::DaemonResponse;
use vibefs::tui;

/// VibeFS - Massively Parallel AI Agent Filesystem
#[derive(Parser)]
#[command(name = "vibe")]
#[command(about = "A virtual filesystem for massively parallel AI agent workflows", long_about = None)]
struct Cli {
    /// Path to the Git repository (defaults to current directory)
    #[arg(short, long, default_value = ".")]
    repo: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize VibeFS for a Git repository
    Init,

    /// Spawn a new vibe workspace
    Spawn {
        /// Vibe ID for the new workspace
        vibe_id: String,
    },

    /// Create a zero-cost snapshot of a vibe session
    Snapshot {
        /// Vibe ID to snapshot
        vibe_id: String,
    },

    /// Promote a vibe session into a Git commit
    Promote {
        /// Vibe ID to promote
        vibe_id: String,
    },

    /// Finalize a vibe into main history
    Commit {
        /// Vibe ID to commit
        vibe_id: String,
    },

    /// Launch the TUI dashboard
    Dashboard,

    /// List files from the virtual filesystem (starts daemon if needed)
    Ls {
        /// Optional path to list
        #[arg(default_value = ".")]
        path: String,

        /// Vibe ID (default session)
        #[arg(short, long, default_value = "default")]
        session: String,
    },

    /// Execute a command in a vibe workspace
    #[command(name = "sh")]
    Shell {
        /// Vibe ID for the workspace
        #[arg(short, long, default_value = "default")]
        session: String,

        /// Command to execute
        #[arg(short, long)]
        command: Option<String>,
    },

    /// Daemon management commands
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    /// Show daemon and session status
    Status,
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon in foreground (for debugging)
    Start,
    /// Stop the running daemon
    Stop,
    /// Show daemon status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let repo_path = cli.repo.canonicalize().unwrap_or(cli.repo.clone());

    match cli.command {
        Commands::Init => {
            commands::init::init(&repo_path).await?;
        }
        Commands::Spawn { vibe_id } => {
            commands::spawn::spawn(&repo_path, &vibe_id).await?;
        }
        Commands::Snapshot { vibe_id } => {
            commands::snapshot::snapshot(&repo_path, &vibe_id).await?;
        }
        Commands::Promote { vibe_id } => {
            commands::promote::promote(&repo_path, &vibe_id).await?;
        }
        Commands::Commit { vibe_id } => {
            commands::commit::commit(&repo_path, &vibe_id).await?;
        }
        Commands::Dashboard => {
            tui::run_dashboard(&repo_path).await?;
        }
        Commands::Ls { path, session: _ } => {
            // Ensure daemon is running
            daemon_client::ensure_daemon_running(&repo_path).await?;

            // For now, just list files from Git HEAD
            // In full implementation, this would use the NFS mount
            let vibe_dir = repo_path.join(".vibe");
            if !vibe_dir.exists() {
                anyhow::bail!("VibeFS not initialized. Run 'vibe init' first.");
            }

            // Use git ls-tree for now
            let output = std::process::Command::new("git")
                .args(["ls-tree", "--name-only", "HEAD", &path])
                .current_dir(&repo_path)
                .output()?;

            if output.status.success() {
                print!("{}", String::from_utf8_lossy(&output.stdout));
            } else {
                anyhow::bail!(
                    "Failed to list files: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        Commands::Shell { session, command } => {
            // Ensure daemon is running and session exists
            daemon_client::ensure_daemon_running(&repo_path).await?;

            let mut client = DaemonClient::connect(&repo_path).await?;

            // Export session if not exists
            match client.export_session(&session).await? {
                DaemonResponse::SessionExported { mount_point, .. } => {
                    if let Some(cmd) = command {
                        // Execute command in mount point
                        let status = std::process::Command::new("sh")
                            .args(["-c", &cmd])
                            .current_dir(&mount_point)
                            .status()?;

                        if !status.success() {
                            std::process::exit(status.code().unwrap_or(1));
                        }
                    } else {
                        // Interactive shell
                        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                        let status = std::process::Command::new(&shell)
                            .current_dir(&mount_point)
                            .status()?;

                        if !status.success() {
                            std::process::exit(status.code().unwrap_or(1));
                        }
                    }
                }
                DaemonResponse::Error { message } => {
                    anyhow::bail!("Daemon error: {}", message);
                }
                _ => {
                    anyhow::bail!("Unexpected daemon response");
                }
            }
        }
        Commands::Daemon { action } => match action {
            DaemonAction::Start => {
                println!("Starting daemon in foreground mode...");
                daemon_client::start_daemon_foreground(&repo_path).await?;
            }
            DaemonAction::Stop => {
                if DaemonClient::is_running(&repo_path).await {
                    let mut client = DaemonClient::connect(&repo_path).await?;
                    client.shutdown().await?;
                    println!("Daemon shutdown requested");
                } else {
                    println!("Daemon is not running");
                }
            }
            DaemonAction::Status => {
                if DaemonClient::is_running(&repo_path).await {
                    let mut client = DaemonClient::connect(&repo_path).await?;
                    match client.status().await? {
                        DaemonResponse::Status {
                            repo_path,
                            nfs_port,
                            session_count,
                            uptime_secs,
                        } => {
                            println!("Daemon Status:");
                            println!("  Repository: {}", repo_path);
                            println!("  NFS Port: {}", nfs_port);
                            println!("  Active Sessions: {}", session_count);
                            println!("  Uptime: {}s", uptime_secs);
                        }
                        _ => {
                            println!("Failed to get daemon status");
                        }
                    }
                } else {
                    println!("Daemon is not running");
                }
            }
        },
        Commands::Status => {
            let vibe_dir = repo_path.join(".vibe");
            if !vibe_dir.exists() {
                println!("VibeFS not initialized. Run 'vibe init' first.");
                return Ok(());
            }

            println!("VibeFS Status");
            println!("=============");
            println!("Repository: {}", repo_path.display());

            // Check daemon status
            if DaemonClient::is_running(&repo_path).await {
                let mut client = DaemonClient::connect(&repo_path).await?;
                match client.status().await? {
                    DaemonResponse::Status {
                        nfs_port,
                        session_count,
                        uptime_secs,
                        ..
                    } => {
                        println!("\nDaemon: Running");
                        println!("  NFS Port: {}", nfs_port);
                        println!("  Uptime: {}s", uptime_secs);
                        println!("  Active Sessions: {}", session_count);

                        // List sessions
                        if let Ok(DaemonResponse::Sessions { sessions }) =
                            client.list_sessions().await
                        {
                            if !sessions.is_empty() {
                                println!("\nSessions:");
                                for session in sessions {
                                    println!(
                                        "  - {} (port: {}, uptime: {}s)",
                                        session.vibe_id, session.nfs_port, session.uptime_secs
                                    );
                                    println!("    Mount: {}", session.mount_point);
                                }
                            }
                        }
                    }
                    _ => {
                        println!("\nDaemon: Unknown status");
                    }
                }
            } else {
                println!("\nDaemon: Not running");
            }

            // List session directories
            let sessions_dir = vibe_dir.join("sessions");
            if sessions_dir.exists() {
                let mut found_sessions = false;
                for entry in std::fs::read_dir(&sessions_dir)? {
                    let entry = entry?;
                    if entry.file_type()?.is_dir() {
                        if !found_sessions {
                            println!("\nLocal Sessions:");
                            found_sessions = true;
                        }
                        println!("  - {}", entry.file_name().to_string_lossy());
                    }
                }
            }
        }
    }

    Ok(())
}
