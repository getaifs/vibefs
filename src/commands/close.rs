//! Close command - close a single session without purging all data

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

use crate::daemon_client::DaemonClient;
use crate::daemon_ipc::DaemonResponse;
use crate::platform;

/// Close a single session, unmounting and cleaning up its data
pub async fn close<P: AsRef<Path>>(
    repo_path: P,
    session_id: &str,
    force: bool,
    show_dirty: bool,
) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");
    let session_dir = vibe_dir.join("sessions").join(session_id);

    if !session_dir.exists() {
        anyhow::bail!("Session '{}' not found", session_id);
    }

    // Show dirty files if requested or before confirmation
    let dirty_files = collect_dirty_files(&session_dir)?;

    if show_dirty || !dirty_files.is_empty() {
        if !dirty_files.is_empty() {
            println!("Dirty files in session '{}':", session_id);
            for file in &dirty_files {
                println!("  {}", file);
            }
            println!();
        } else {
            println!("No dirty files in session '{}'", session_id);
        }
    }

    if show_dirty {
        // Just showing dirty files, don't close
        return Ok(());
    }

    if !force && !dirty_files.is_empty() {
        print!(
            "Session '{}' has {} dirty file(s). Close anyway? [y/N] ",
            session_id,
            dirty_files.len()
        );
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Unmount via daemon if running
    if DaemonClient::is_running(repo_path).await {
        let mut client = DaemonClient::connect(repo_path).await?;
        match client.unexport_session(session_id).await? {
            DaemonResponse::SessionUnexported { .. } => {
                println!("Unmounted session '{}'", session_id);
            }
            DaemonResponse::Error { message } => {
                // Session might not be mounted, continue with cleanup
                eprintln!("Note: {}", message);
            }
            _ => {}
        }
    }

    // Get repo name for mount point
    let repo_name = repo_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());

    // Force unmount the mount point - try both new and legacy formats
    let mounts_dir = platform::get_vibe_mounts_dir();
    let mount_points = vec![
        // New format: repo_name-session_id
        mounts_dir.join(format!("{}-{}", repo_name, session_id)),
        // Legacy format: just session_id
        mounts_dir.join(session_id),
    ];

    for mount_point in mount_points {
        if mount_point.exists() {
            println!("Unmounting {}...", mount_point.display());

            #[cfg(target_os = "macos")]
            {
                // Try diskutil unmount force first
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    tokio::process::Command::new("diskutil")
                        .args(["unmount", "force"])
                        .arg(&mount_point)
                        .output(),
                )
                .await;

                // Fallback to umount -f
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    tokio::process::Command::new("umount")
                        .arg("-f")
                        .arg(&mount_point)
                        .output(),
                )
                .await;
            }

            #[cfg(target_os = "linux")]
            {
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    tokio::process::Command::new("umount")
                        .arg("-l")
                        .arg(&mount_point)
                        .output(),
                )
                .await;
            }

            // Remove mount point directory
            if let Err(e) = std::fs::remove_dir(&mount_point) {
                eprintln!("Warning: Failed to remove mount point: {}", e);
            }
        }
    }

    // Remove session directory
    println!("Removing session directory...");
    std::fs::remove_dir_all(&session_dir)
        .with_context(|| format!("Failed to remove session directory: {}", session_dir.display()))?;

    // Also remove any spawn info json file
    let spawn_info = vibe_dir.join("sessions").join(format!("{}.json", session_id));
    if spawn_info.exists() {
        std::fs::remove_file(&spawn_info).ok();
    }

    println!("Session '{}' closed successfully", session_id);
    Ok(())
}

/// Collect dirty files in a session directory
fn collect_dirty_files(session_dir: &Path) -> Result<Vec<String>> {
    let mut dirty = Vec::new();
    collect_files_recursive(session_dir, session_dir, &mut dirty)?;
    Ok(dirty)
}

fn collect_files_recursive(
    base: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> Result<()> {
    if !current.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        // Skip macOS resource fork files (._filename) and .DS_Store
        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy();
            if name_str.starts_with("._") || name_str == ".DS_Store" {
                continue;
            }
        }

        if path.is_dir() {
            collect_files_recursive(base, &path, files)?;
        } else {
            // Get relative path from session dir
            if let Ok(rel) = path.strip_prefix(base) {
                files.push(rel.display().to_string());
            }
        }
    }

    Ok(())
}

/// List dirty files for a session without closing
pub async fn list_dirty<P: AsRef<Path>>(repo_path: P, session_id: &str) -> Result<Vec<String>> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");
    let session_dir = vibe_dir.join("sessions").join(session_id);

    if !session_dir.exists() {
        anyhow::bail!("Session '{}' not found", session_id);
    }

    collect_dirty_files(&session_dir)
}
