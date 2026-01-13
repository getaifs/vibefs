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
        /// Vibe ID for the new workspace (auto-generated if not provided)
        vibe_id: Option<String>,
    },

    /// Create a zero-cost snapshot of a vibe session
    Snapshot {
        /// Vibe ID to snapshot
        vibe_id: String,
    },

    /// Restore session state from a snapshot
    Restore {
        /// Session ID to restore
        session: String,

        /// Snapshot name to restore from
        #[arg(long)]
        snapshot: String,

        /// Skip automatic backup of current state
        #[arg(long)]
        no_backup: bool,
    },

    /// Promote a vibe session into a Git commit
    Promote {
        /// Vibe ID to promote
        vibe_id: String,
    },

    /// Close a vibe session (unmount and clean up)
    Close {
        /// Vibe ID to close
        session: String,

        /// Force close without confirmation (even with dirty files)
        #[arg(short, long)]
        force: bool,

        /// Only show dirty files, don't close the session
        #[arg(long)]
        dirty: bool,
    },

    /// Get the mount path for an existing vibe session
    Path {
        /// Vibe ID (must already exist)
        session: String,
    },

    /// Launch the TUI dashboard
    Dashboard,

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

    /// Inspect session metadata for debugging
    Inspect {
        /// Session ID to inspect
        session: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show unified diff of session changes
    Diff {
        /// Session ID to show diff for
        session: String,

        /// Show diffstat summary only
        #[arg(long)]
        stat: bool,

        /// Color output: auto, always, never
        #[arg(long, default_value = "auto")]
        color: String,

        /// Disable pager (less)
        #[arg(long)]
        no_pager: bool,
    },

    /// Show daemon and session status
    Status {
        /// Show details for a specific session
        session: Option<String>,

        /// Show cross-session file conflicts
        #[arg(long)]
        conflicts: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Clean up VibeFS data (all or specific session)
    Purge {
        /// Specific session to purge (if not specified, purges all)
        #[arg(short, long)]
        session: Option<String>,

        /// Force purge without confirmation
        #[arg(short, long)]
        force: bool,
    },
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
            let vibe_id = vibe_id.unwrap_or_else(|| {
                let sessions_dir = repo_path.join(".vibe/sessions");
                vibefs::names::generate_unique_name(&sessions_dir)
            });
            commands::spawn::spawn(&repo_path, &vibe_id).await?;
        }
        Commands::Snapshot { vibe_id } => {
            commands::snapshot::snapshot(&repo_path, &vibe_id).await?;
        }
        Commands::Restore { session, snapshot, no_backup } => {
            commands::restore::restore(&repo_path, &session, &snapshot, no_backup).await?;
        }
        Commands::Promote { vibe_id } => {
            commands::promote::promote(&repo_path, &vibe_id).await?;
        }
        Commands::Close { session, force, dirty } => {
            commands::close::close(&repo_path, &session, force, dirty).await?;
        }
        Commands::Path { session } => {
            // Check if session exists - do NOT auto-create
            let vibe_dir = repo_path.join(".vibe");
            let session_dir = vibe_dir.join("sessions").join(&session);

            if !session_dir.exists() {
                anyhow::bail!(
                    "Session '{}' does not exist. Use 'vibe spawn {}' to create it.",
                    session,
                    session
                );
            }

            // Check if daemon is running and session is mounted
            if !DaemonClient::is_running(&repo_path).await {
                anyhow::bail!(
                    "Daemon not running. Session '{}' exists but is not mounted.\n\
                     Use 'vibe spawn {}' to start the daemon and mount it.",
                    session,
                    session
                );
            }

            let mut client = DaemonClient::connect(&repo_path).await?;

            // List sessions to find this one
            match client.list_sessions().await? {
                DaemonResponse::Sessions { sessions } => {
                    if let Some(sess) = sessions.iter().find(|s| s.vibe_id == session) {
                        println!("{}", sess.mount_point);
                    } else {
                        anyhow::bail!(
                            "Session '{}' exists but is not mounted.\n\
                             Use 'vibe spawn {}' to mount it.",
                            session,
                            session
                        );
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
        Commands::Inspect { session, json } => {
            commands::inspect::inspect(&repo_path, &session, json).await?;
        }
        Commands::Diff { session, stat, color, no_pager } => {
            let color_opt = color.parse().unwrap_or(commands::diff::ColorOption::Auto);
            commands::diff::diff(&repo_path, &session, stat, color_opt, no_pager).await?;
        }
        Commands::Dashboard => {
            tui::run_dashboard(&repo_path).await?;
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
        Commands::Status { session, conflicts, json } => {
            commands::status::status(&repo_path, session.as_deref(), conflicts, json).await?;
        }
        Commands::Purge { session, force } => {
            if let Some(session_id) = session {
                // Close a specific session
                commands::close::close(&repo_path, &session_id, force, false).await?;
            } else {
                // Purge all
                commands::purge::purge(&repo_path, force).await?;
            }
        }
    }

    Ok(())
}
