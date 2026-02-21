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
    Init,

    /// Create a new session and enter shell
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
    Save {
        /// Snapshot name/message (auto-generated timestamp if not provided)
        #[arg(short = 'm', long = "message", alias = "name")]
        message: Option<String>,

        /// Session to snapshot (auto-detected if in mount or single session)
        #[arg(short, long)]
        session: Option<String>,
    },

    /// Restore session from a checkpoint
    Undo {
        /// Snapshot name/message to restore (lists available if not provided)
        #[arg(short = 'm', long = "message", alias = "name")]
        message: Option<String>,

        /// Session to restore (auto-detected if in mount or single session)
        #[arg(short, long)]
        session: Option<String>,

        /// Skip automatic backup of current state
        #[arg(long)]
        no_backup: bool,
    },

    /// Rebase session to current HEAD (update base commit)
    Rebase {
        /// Session to rebase (auto-detected if in mount or single session)
        session: Option<String>,

        /// Force rebase even if there are potential conflicts
        #[arg(short, long)]
        force: bool,
    },

    /// Commit session changes to a Git ref
    #[command(alias = "promote")]
    Commit {
        /// Session to commit (auto-detected if in mount or single session)
        #[arg(short, long)]
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
    #[command(alias = "close")]
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
    Attach {
        /// Session to attach to (auto-detected if possible)
        session: Option<String>,

        /// Command to execute instead of interactive shell
        #[arg(short, long)]
        command: Option<String>,
    },

    /// Daemon management commands
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    /// Show unified diff of session changes
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
    #[command(name = "ls", alias = "status")]
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

    /// Agent shortcut (e.g., 'vibe claude' -> 'vibe new --agent claude')
    #[command(external_subcommand)]
    Agent(Vec<String>),
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
        Commands::Save { message, session } => {
            let session = commands::require_session(&repo_path, session)?;
            // Generate timestamp name if not provided
            let snapshot_name = message.unwrap_or_else(|| {
                chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
            });
            commands::snapshot::snapshot_with_name(&repo_path, &session, &snapshot_name).await?;
        }
        Commands::Undo { message, session, no_backup } => {
            let session = commands::require_session(&repo_path, session)?;
            if let Some(snapshot_name) = message {
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
        Commands::Agent(args) => {
            // Check if first arg is a known agent
            if let Some(agent) = args.first() {
                if commands::launch::is_known_agent(agent) {
                    // Auto-init if .vibe/ doesn't exist
                    let vibe_dir = repo_path.join(".vibe");
                    if !vibe_dir.exists() {
                        commands::init::init(&repo_path).await?;
                    }
                    // Pass remaining args to the agent
                    let agent_args: Vec<String> = args.iter().skip(1).cloned().collect();
                    commands::launch::launch(&repo_path, agent, None, &agent_args).await?;
                } else {
                    // Unknown command - show helpful error
                    let known = commands::launch::KNOWN_AGENTS.join(", ");
                    anyhow::bail!(
                        "Unknown command '{}'\n\n\
                         Known agent shortcuts: {}\n\n\
                         Run 'vibe --help' to see available commands.",
                        agent,
                        known
                    );
                }
            } else {
                anyhow::bail!("No command provided. Run 'vibe --help' for usage.");
            }
        }
    }

    Ok(())
}
