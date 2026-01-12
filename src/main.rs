use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use vibefs::commands;
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
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            commands::init::init(&cli.repo).await?;
        }
        Commands::Spawn { vibe_id } => {
            commands::spawn::spawn(&cli.repo, &vibe_id).await?;
        }
        Commands::Snapshot { vibe_id } => {
            commands::snapshot::snapshot(&cli.repo, &vibe_id).await?;
        }
        Commands::Promote { vibe_id } => {
            commands::promote::promote(&cli.repo, &vibe_id).await?;
        }
        Commands::Commit { vibe_id } => {
            commands::commit::commit(&cli.repo, &vibe_id).await?;
        }
        Commands::Dashboard => {
            tui::run_dashboard(&cli.repo).await?;
        }
    }

    Ok(())
}
