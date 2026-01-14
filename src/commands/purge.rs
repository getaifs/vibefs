use anyhow::{Context, Result};
use std::path::Path;
use std::io::Write;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use crate::daemon_client::DaemonClient;
use crate::daemon_ipc::get_pid_path;
use crate::platform;

pub async fn purge<P: AsRef<Path>>(repo_path: P, force: bool) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    if !vibe_dir.exists() {
        println!("No VibeFS data found at {}", vibe_dir.display());
        return Ok(());
    }

    if !force {
        print!("Are you sure you want to delete all VibeFS data for this repo? This includes all active sessions and cannot be undone. [y/N] ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    println!("Stopping daemon...");
    
    // Try graceful shutdown via IPC
    if DaemonClient::is_running(repo_path).await {
         if let Ok(mut client) = DaemonClient::connect(repo_path).await {
             println!("  Sending shutdown signal...");
             let _ = client.shutdown().await;
             // Give it a moment to clean up
             tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
         }
    }

    // Check PID file and force kill if still running
    let pid_path = get_pid_path(repo_path);
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid_val) = pid_str.trim().parse::<i32>() {
                if pid_val > 0 {
                    let pid = Pid::from_raw(pid_val);
                    // Check if process exists (kill with signal 0)
                    if kill(pid, None).is_ok() {
                        println!("  Daemon still running (PID {}), forcing shutdown...", pid_val);
                        let _ = kill(pid, Signal::SIGTERM);
                        
                        // Wait a bit
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                        
                        // Force kill if necessary
                        if kill(pid, None).is_ok() {
                            let _ = kill(pid, Signal::SIGKILL);
                        }
                    }
                }
            }
        }
    }

    // Try to unmount any lingering mounts in ~/Library/Caches/vibe/mounts/
    // This is best-effort. The daemon *should* have cleaned up, but we are purging because things are likely broken.
    // We can iterate over sessions dir to find IDs.
    let sessions_dir = vibe_dir.join("sessions");

    // Get repo name for mount point pattern
    let repo_name = repo_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());

    if sessions_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let vibe_id = entry.file_name();
                        let vibe_id_str = vibe_id.to_string_lossy();

                        // Try both old format (just vibe_id) and new format (repo_name-vibe_id)
                        let mounts_dir = platform::get_vibe_mounts_dir();
                        let mount_points = vec![
                            // New format: repo_name-vibe_id
                            mounts_dir.join(format!("{}-{}", repo_name, vibe_id_str)),
                            // Legacy format for backwards compatibility
                            mounts_dir.join(vibe_id_str.to_string()),
                        ];

                        for mount_point in mount_points {
                            if mount_point.exists() {
                                println!("  Unmounting {}...", mount_point.display());

                                #[cfg(target_os = "macos")]
                                {
                                    // Try diskutil unmount force first (usually better for stuck mounts)
                                    let _ = tokio::time::timeout(
                                        std::time::Duration::from_secs(5),
                                        tokio::process::Command::new("diskutil")
                                            .args(["unmount", "force"])
                                            .arg(&mount_point)
                                            .output()
                                    ).await;

                                    // Fallback to umount -f
                                    let _ = tokio::time::timeout(
                                        std::time::Duration::from_secs(5),
                                        tokio::process::Command::new("umount")
                                            .arg("-f")
                                            .arg(&mount_point)
                                            .output()
                                    ).await;
                                }

                                #[cfg(target_os = "linux")]
                                {
                                    let _ = tokio::time::timeout(
                                        std::time::Duration::from_secs(5),
                                        tokio::process::Command::new("umount")
                                            .arg("-l") // Lazy unmount
                                            .arg(&mount_point)
                                            .output()
                                    ).await;
                                }

                                // Try removing the mount point directory
                                if let Err(e) = std::fs::remove_dir(&mount_point) {
                                    // If it fails (e.g. still mounted), we just warn
                                    println!("  Warning: Failed to remove mount point: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!("Removing .vibe directory...");
    std::fs::remove_dir_all(&vibe_dir).context("Failed to remove .vibe directory")?;

    println!("✓ VibeFS purged successfully");
    Ok(())
}
