use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use vibefs::commands;
use vibefs::daemon_client::{self, DaemonClient};
use vibefs::daemon_ipc::DaemonResponse;

/// Build version string with git hash
fn version_string() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION"),
        " (",
        env!("GIT_HASH"),
        ")"
    )
}

/// VibeFS - Massively Parallel AI Agent Filesystem
#[derive(Parser)]
#[command(name = "vibe")]
#[command(version = version_string())]
#[command(about = "A virtual filesystem for massively parallel AI agent workflows", long_about = None)]
#[command(help_template = "\
{about}

Usage: {name} [OPTIONS] [COMMAND]

Sessions:
  new       Create a new session and enter shell
  attach    Attach to an existing session
  kill      Kill a session (unmount and clean up)

Versioning:
  save      Create a checkpoint of session state
  undo      Restore from checkpoint, or reset (--hard)
  commit    Commit session changes to a Git branch
  diff      Show unified diff of session changes

Info:
  ls        List sessions and show status

System:
  init      Initialize VibeFS for a Git repository
  rebase    Rebase session to current HEAD
  daemon    Daemon management commands

{all-args}\
")]
struct Cli {
    /// Path to the Git repository (defaults to current directory)
    #[arg(short, long, default_value = ".")]
    repo: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize VibeFS for a Git repository
    #[command(hide = true)]
    Init,

    /// Create a new session and enter shell
    #[command(hide = true)]
    New {
        /// Session name (auto-generated if not provided)
        session: Option<String>,

        /// Command to execute instead of interactive shell
        #[arg(short, long)]
        command: Option<String>,

        /// Launch an agent (claude, cursor, aider, etc.) instead of shell
        #[arg(long)]
        agent: Option<String>,

        /// Additional arguments to pass to the agent (use after --)
        #[arg(last = true)]
        agent_args: Vec<String>,
    },

    /// Create a checkpoint of session state
    #[command(hide = true)]
    Save {
        /// Snapshot name (auto-generated timestamp if not provided)
        name: Option<String>,

        /// Session to snapshot (auto-detected if in mount or single session)
        #[arg(short, long)]
        session: Option<String>,
    },

    /// Restore session from a checkpoint
    #[command(alias = "restore", hide = true)]
    Undo {
        /// Snapshot name to restore (lists available if not provided)
        name: Option<String>,

        /// Session to restore (auto-detected if in mount or single session)
        #[arg(short, long)]
        session: Option<String>,

        /// Skip automatic backup of current state
        #[arg(long)]
        no_backup: bool,

        /// Discard all changes and reset session to base commit
        #[arg(long)]
        hard: bool,
    },

    /// Rebase session to current HEAD (update base commit)
    #[command(hide = true)]
    Rebase {
        /// Session to rebase (auto-detected if in mount or single session)
        session: Option<String>,

        /// Force rebase even if there are potential conflicts
        #[arg(short, long)]
        force: bool,
    },

    /// Commit session changes to a Git ref
    #[command(alias = "promote", hide = true)]
    Commit {
        /// Session to commit (auto-detected if in mount or single session)
        session: Option<String>,

        /// Commit all sessions with dirty files
        #[arg(short, long)]
        all: bool,

        /// Only commit files matching these glob patterns
        #[arg(long, value_delimiter = ',')]
        only: Option<Vec<String>>,

        /// Custom commit message
        #[arg(short, long)]
        message: Option<String>,
    },

    /// Kill a session (unmount and clean up)
    #[command(alias = "close", hide = true)]
    Kill {
        /// Session to kill (auto-detected if in mount or single session)
        session: Option<String>,

        /// Force kill without confirmation (even with dirty files)
        #[arg(short, long)]
        force: bool,

        /// Kill all sessions
        #[arg(short, long)]
        all: bool,

        /// Also delete the .vibe directory entirely (use with --all)
        #[arg(long)]
        purge: bool,
    },

    /// Attach to an existing session (enter shell at mount point)
    #[command(hide = true)]
    Attach {
        /// Session to attach to (auto-detected if possible)
        session: Option<String>,

        /// Command to execute instead of interactive shell
        #[arg(short, long)]
        command: Option<String>,
    },

    /// Daemon management commands
    #[command(hide = true)]
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    /// Show unified diff of session changes
    #[command(hide = true)]
    Diff {
        /// Session ID to show diff for (auto-detected if in mount or single session)
        session: Option<String>,

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

    /// List sessions and show status
    #[command(name = "ls", alias = "status", hide = true)]
    Ls {
        /// Show details for a specific session (auto-detected if in mount or single session)
        session: Option<String>,

        /// Show cross-session file conflicts
        #[arg(long)]
        conflicts: bool,

        /// Show verbose debug information
        #[arg(short, long)]
        verbose: bool,

        /// Print only the mount path (for scripting)
        #[arg(short, long)]
        path: bool,

        /// Output as JSON
        #[arg(short = 'J', long)]
        json: bool,
    },

}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon (background by default)
    Start {
        /// Run in foreground for debugging
        #[arg(short, long)]
        foreground: bool,
    },
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
    let repo_path = vibefs::platform::get_effective_repo_path(&cli.repo);

    // Handle no subcommand: show status overview by default
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            commands::status::status(&repo_path, None, false, false).await?;
            return Ok(());
        }
    };

    match command {
        Commands::Init => {
            commands::init::init(&repo_path).await?;
        }
        Commands::New { session, command, agent, agent_args } => {
            // Auto-init if .vibe/ doesn't exist
            let vibe_dir = repo_path.join(".vibe");
            if !vibe_dir.exists() {
                commands::init::init(&repo_path).await?;
            }

            // Generate session name if not provided
            let session = session.unwrap_or_else(|| {
                let sessions_dir = repo_path.join(".vibe/sessions");
                vibefs::names::generate_unique_name(&sessions_dir)
            });

            // If agent is specified, delegate to launch
            if let Some(agent_name) = agent {
                commands::launch::launch(&repo_path, &agent_name, Some(&session), &agent_args).await?;
            } else {
                // Spawn the session
                commands::spawn::spawn(&repo_path, &session).await?;

                // Connect to daemon and enter shell
                let mut client = DaemonClient::connect(&repo_path).await?;
                match client.export_session(&session).await? {
                    DaemonResponse::SessionExported { mount_point, nfs_port, .. } => {
                        // Ensure NFS is mounted
                        if let Err(e) = commands::spawn::mount_nfs(&mount_point, nfs_port) {
                            eprintln!("Warning: mount issue: {}", e);
                        }

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
        }
        Commands::Save { name, session } => {
            let session = commands::require_session(&repo_path, session)?;
            // Generate timestamp name if not provided
            let snapshot_name = name.unwrap_or_else(|| {
                chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
            });
            commands::snapshot::snapshot_with_name(&repo_path, &session, &snapshot_name).await?;
        }
        Commands::Undo { name, session, no_backup, hard } => {
            let session = commands::require_session(&repo_path, session)?;
            if hard {
                commands::restore::reset_hard(&repo_path, &session, no_backup).await?;
            } else if let Some(snapshot_name) = name {
                commands::restore::restore(&repo_path, &session, &snapshot_name, no_backup).await?;
            } else {
                // List available snapshots
                commands::snapshot::list_snapshots(&repo_path, &session).await?;
            }
        }
        Commands::Rebase { session, force } => {
            let session = commands::require_session(&repo_path, session)?;
            commands::rebase::rebase(&repo_path, &session, force).await?;
        }
        Commands::Commit { session, all, only, message } => {
            if all {
                commands::promote::promote_all(&repo_path, message.as_deref()).await?;
            } else {
                let id = commands::require_session(&repo_path, session)?;
                commands::promote::promote(&repo_path, &id, only, message.as_deref()).await?;
            }
        }
        Commands::Kill { session, force, all, purge } => {
            if all {
                commands::purge::purge(&repo_path, force).await?;
                if purge {
                    let vibe_dir = repo_path.join(".vibe");
                    if vibe_dir.exists() {
                        std::fs::remove_dir_all(&vibe_dir)?;
                        println!("Removed .vibe directory");
                    }
                }
            } else {
                let session = commands::require_session(&repo_path, session)?;
                commands::close::close(&repo_path, &session, force, false).await?;
            }
        }
        Commands::Attach { session, command } => {
            let session = commands::require_session(&repo_path, session)?;

            // Ensure daemon is running and session is exported
            daemon_client::ensure_daemon_running(&repo_path).await?;
            let mut client = DaemonClient::connect(&repo_path).await?;
            match client.export_session(&session).await? {
                DaemonResponse::SessionExported { mount_point, nfs_port, .. } => {
                    // Ensure NFS is mounted
                    if let Err(e) = commands::spawn::mount_nfs(&mount_point, nfs_port) {
                        eprintln!("Warning: mount issue: {}", e);
                    }

                    if let Some(cmd) = command {
                        let status = std::process::Command::new("sh")
                            .args(["-c", &cmd])
                            .current_dir(&mount_point)
                            .status()?;
                        if !status.success() {
                            std::process::exit(status.code().unwrap_or(1));
                        }
                    } else {
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
        Commands::Diff { session, stat, color, no_pager } => {
            let session = commands::require_session(&repo_path, session)?;
            let color_opt = color.parse().unwrap_or(commands::diff::ColorOption::Auto);
            commands::diff::diff(&repo_path, &session, stat, color_opt, no_pager).await?;
        }
        Commands::Daemon { action } => match action {
            DaemonAction::Start { foreground } => {
                if foreground {
                    println!("Starting daemon in foreground mode...");
                    daemon_client::start_daemon_foreground(&repo_path).await?;
                } else {
                    daemon_client::ensure_daemon_running(&repo_path).await?;
                    println!("Daemon started.");
                }
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
                            version,
                        } => {
                            println!("Daemon Status:");
                            println!("  Repository: {}", repo_path);
                            if let Some(v) = version {
                                println!("  Version: {}", v);
                            }
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
        Commands::Ls { session, conflicts, verbose, path, json } => {
            if path {
                // Path-only mode: print mount path for scripting
                let session = commands::require_session(&repo_path, session)?;
                let spawn_info = commands::spawn::SpawnInfo::load(&repo_path, &session)?;
                println!("{}", spawn_info.mount_point.display());
            } else if verbose {
                // Verbose mode: use inspect logic for detailed debug info
                let session = commands::require_session(&repo_path, session)?;
                commands::inspect::inspect(&repo_path, &session, json).await?;
            } else {
                commands::status::status(&repo_path, session.as_deref(), conflicts, json).await?;
            }
        }
    }

    Ok(())
}
