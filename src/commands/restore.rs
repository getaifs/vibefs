//! `vibe restore` command - Restore session state from a snapshot

use anyhow::{Context, Result};
use chrono::Utc;
use std::path::Path;

use crate::cwd_validation;
use crate::daemon_client::DaemonClient;
use crate::daemon_ipc::DaemonResponse;
use crate::db::MetadataStore;
use crate::platform;

/// Restore session state from a snapshot
pub async fn restore<P: AsRef<Path>>(
    repo_path: P,
    session: &str,
    snapshot_name: &str,
    no_backup: bool,
) -> Result<()> {
    // Validate that we're running from the correct directory
    let _validated_root = cwd_validation::validate_cwd()
        .context("Cannot restore snapshot")?;

    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");
    let sessions_dir = vibe_dir.join("sessions");
    let session_dir = sessions_dir.join(session);

    // Verify session exists
    if !session_dir.exists() {
        anyhow::bail!(
            "Session '{}' not found. Run 'vibe status' to see active sessions.",
            session
        );
    }

    // Find snapshot - try both formats
    let snapshot_dir = find_snapshot(&sessions_dir, session, snapshot_name)?;

    println!("Restoring session '{}' from snapshot '{}'", session, snapshot_name);

    // Auto-backup current state before restore (unless --no-backup)
    if !no_backup {
        let backup_name = format!("pre-restore-{}", Utc::now().format("%Y%m%d_%H%M%S"));
        let backup_dir = sessions_dir.join(format!("{}_snapshot_{}", session, backup_name));

        println!("  Backing up current state to snapshot '{}'...", backup_name);

        #[cfg(target_os = "macos")]
        {
            copy_with_clonefile(&session_dir, &backup_dir)?;
        }

        #[cfg(target_os = "linux")]
        {
            copy_with_reflink(&session_dir, &backup_dir)?;
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            copy_recursive(&session_dir, &backup_dir)?;
        }

        println!("  Backed up current state to snapshot '{}'", backup_name);
    }

    // If daemon is running, unmount and unexport the session first
    // so we can acquire the metadata.db lock for dirty tracking updates
    let daemon_running = DaemonClient::is_running(repo_path).await;
    let mount_point = if daemon_running {
        // Compute mount point path (same logic as daemon)
        let repo_name = repo_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".to_string());
        let mp = platform::get_vibe_mounts_dir()
            .join(format!("{}-{}", repo_name, session));

        // Force unmount first so NFS client disconnects and the server task can exit
        if mp.exists() {
            platform::unmount_nfs_sync(&mp.to_string_lossy()).ok();
        }

        // Unexport from daemon (stops NFS server, releases metadata.db lock)
        let mut client = DaemonClient::connect(repo_path).await?;
        match client.unexport_session(session).await? {
            DaemonResponse::SessionUnexported { .. } => {}
            DaemonResponse::Error { message } => {
                eprintln!("Warning: unexport failed: {}", message);
            }
            _ => {}
        }

        Some(mp)
    } else {
        None
    };

    // Delete current session delta
    println!("  Removing current session state...");
    std::fs::remove_dir_all(&session_dir)
        .with_context(|| format!("Failed to remove session directory: {}", session_dir.display()))?;

    // Copy snapshot to session
    println!("  Restoring from snapshot...");

    #[cfg(target_os = "macos")]
    {
        copy_with_clonefile(&snapshot_dir, &session_dir)?;
    }

    #[cfg(target_os = "linux")]
    {
        copy_with_reflink(&snapshot_dir, &session_dir)?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        copy_recursive(&snapshot_dir, &session_dir)?;
    }

    // Clear and rebuild dirty tracking.
    // When daemon is running, use the per-session metadata.db (restored from snapshot).
    // The base .vibe/metadata.db is still locked by the daemon — don't touch it.
    // When daemon is not running, use the base metadata.db.
    let db_path = if daemon_running {
        session_dir.join("metadata.db")
    } else {
        vibe_dir.join("metadata.db")
    };
    if db_path.exists() {
        println!("  Updating dirty file tracking...");
        let store = MetadataStore::open(&db_path)
            .context("Failed to open metadata store")?;

        // Clear existing dirty markers
        store.clear_dirty()?;

        // Re-scan restored files and mark as dirty
        mark_files_dirty(&session_dir, &store, "")?;

        // Drop the store explicitly before re-export so daemon can reacquire
        drop(store);
    }

    // Re-export session if daemon was running
    if let Some(mp) = mount_point {
        let mut client = DaemonClient::connect(repo_path).await?;
        match client.export_session(session).await? {
            DaemonResponse::SessionExported { mount_point, nfs_port, .. } => {
                if let Err(e) = platform::mount_nfs(&mount_point, nfs_port) {
                    eprintln!("Warning: mount issue: {}", e);
                }
            }
            DaemonResponse::Error { message } => {
                eprintln!("Warning: re-export failed: {}. Re-export manually with: vibe export {}", message, session);
            }
            _ => {}
        }

        // Clean up stale mount point if it's different from the new one
        if mp.exists() {
            std::fs::remove_dir(&mp).ok();
        }
    }

    println!("✓ Session '{}' restored from snapshot '{}'", session, snapshot_name);

    Ok(())
}

/// Discard all session changes and reset to base commit (clean state)
pub async fn reset_hard<P: AsRef<Path>>(
    repo_path: P,
    session: &str,
    no_backup: bool,
) -> Result<()> {
    let _validated_root = cwd_validation::validate_cwd()
        .context("Cannot reset session")?;

    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");
    let sessions_dir = vibe_dir.join("sessions");
    let session_dir = sessions_dir.join(session);

    if !session_dir.exists() {
        anyhow::bail!(
            "Session '{}' not found. Run 'vibe ls' to see active sessions.",
            session
        );
    }

    // Auto-backup current state before reset (unless --no-backup)
    if !no_backup {
        let backup_name = format!("pre-reset-{}", Utc::now().format("%Y%m%d_%H%M%S"));
        let backup_dir = sessions_dir.join(format!("{}_snapshot_{}", session, backup_name));

        println!("Backing up current state to checkpoint '{}'...", backup_name);

        #[cfg(target_os = "macos")]
        {
            copy_with_clonefile(&session_dir, &backup_dir)?;
        }

        #[cfg(target_os = "linux")]
        {
            copy_with_reflink(&session_dir, &backup_dir)?;
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            copy_recursive(&session_dir, &backup_dir)?;
        }
    }

    // If daemon is running, unmount and unexport the session first
    let daemon_running = DaemonClient::is_running(repo_path).await;
    let mount_point = if daemon_running {
        // Compute mount point path (same logic as daemon)
        let repo_name = repo_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".to_string());
        let mp = platform::get_vibe_mounts_dir()
            .join(format!("{}-{}", repo_name, session));

        // Force unmount first so NFS client disconnects and the server task can exit
        if mp.exists() {
            platform::unmount_nfs_sync(&mp.to_string_lossy()).ok();
        }

        // Unexport from daemon (stops NFS server, releases metadata.db lock)
        let mut client = DaemonClient::connect(repo_path).await?;
        match client.unexport_session(session).await? {
            DaemonResponse::SessionUnexported { .. } => {}
            DaemonResponse::Error { message } => {
                eprintln!("Warning: unexport failed: {}", message);
            }
            _ => {}
        }

        Some(mp)
    } else {
        None
    };

    // Remove all files in session dir (including metadata.db — it will be
    // re-cloned from the base on next export, with clean dirty markers)
    println!("Discarding all changes in session '{}'...", session);
    for entry in std::fs::read_dir(&session_dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_symlink() || ft.is_file() {
            std::fs::remove_file(&path)?;
        } else if ft.is_dir() {
            std::fs::remove_dir_all(&path)?;
        }
    }

    // Re-export session if daemon was running
    if let Some(mp) = mount_point {
        let mut client = DaemonClient::connect(repo_path).await?;
        match client.export_session(session).await? {
            DaemonResponse::SessionExported { mount_point, nfs_port, .. } => {
                if let Err(e) = platform::mount_nfs(&mount_point, nfs_port) {
                    eprintln!("Warning: mount issue: {}", e);
                }
            }
            DaemonResponse::Error { message } => {
                eprintln!("Warning: re-export failed: {}. Re-export manually with: vibe export {}", message, session);
            }
            _ => {}
        }

        // Clean up stale mount point if it's different from the new one
        if mp.exists() {
            std::fs::remove_dir(&mp).ok();
        }
    }

    println!("Session '{}' reset to base commit.", session);
    if !no_backup {
        println!("  (backup saved — use 'vibe undo' to see checkpoints)");
    }

    Ok(())
}

/// Find a snapshot by name (handles different naming formats)
fn find_snapshot(sessions_dir: &Path, session: &str, snapshot_name: &str) -> Result<std::path::PathBuf> {
    // Try exact match first: <session>_snapshot_<name>
    let full_name = format!("{}_snapshot_{}", session, snapshot_name);
    let snapshot_dir = sessions_dir.join(&full_name);
    if snapshot_dir.exists() {
        return Ok(snapshot_dir);
    }

    // Try just the name in case user provided full snapshot name
    let snapshot_dir = sessions_dir.join(snapshot_name);
    if snapshot_dir.exists() && snapshot_name.starts_with(&format!("{}_snapshot_", session)) {
        return Ok(snapshot_dir);
    }

    // Search for partial match
    let prefix = format!("{}_snapshot_{}", session, snapshot_name);
    for entry in std::fs::read_dir(sessions_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) {
            return Ok(entry.path());
        }
    }

    // List available snapshots for error message
    let available: Vec<String> = std::fs::read_dir(sessions_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with(&format!("{}_snapshot_", session)))
        .map(|name| {
            // Extract just the snapshot name part
            name.strip_prefix(&format!("{}_snapshot_", session))
                .unwrap_or(&name)
                .to_string()
        })
        .collect();

    if available.is_empty() {
        anyhow::bail!(
            "Snapshot '{}' not found for session '{}'. No snapshots exist for this session.\n\
             Create one with: vibe snapshot {}",
            snapshot_name,
            session,
            session
        );
    } else {
        anyhow::bail!(
            "Snapshot '{}' not found for session '{}'. Available snapshots:\n  {}",
            snapshot_name,
            session,
            available.join("\n  ")
        );
    }
}

/// Recursively mark all files in directory as dirty
fn mark_files_dirty(dir: &Path, store: &MetadataStore, prefix: &str) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().to_string();

        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", prefix, name)
        };

        if file_type.is_dir() {
            mark_files_dirty(&entry.path(), store, &path)?;
        } else {
            store.mark_dirty(&path)?;
        }
    }

    Ok(())
}

// Platform-specific copy functions (same as snapshot.rs)

#[cfg(target_os = "macos")]
fn copy_with_clonefile(src: &Path, dst: &Path) -> Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let src_cstr = CString::new(src.as_os_str().as_bytes())?;
    let dst_cstr = CString::new(dst.as_os_str().as_bytes())?;

    let result = unsafe {
        libc::clonefile(
            src_cstr.as_ptr(),
            dst_cstr.as_ptr(),
            0,
        )
    };

    if result != 0 {
        anyhow::bail!("clonefile failed: {}", std::io::Error::last_os_error());
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn copy_with_reflink(src: &Path, dst: &Path) -> Result<()> {
    use std::process::Command;

    let output = Command::new("cp")
        .arg("-r")
        .arg("--reflink=auto")
        .arg(src)
        .arg(dst)
        .output()
        .context("Failed to execute cp with reflink")?;

    if !output.status.success() {
        copy_recursive(src, dst)?;
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{init, snapshot, spawn};
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        fs::write(repo_path.join("README.md"), "# Test").unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        temp_dir
    }

    #[tokio::test]
    async fn test_restore_creates_backup() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Initialize and spawn
        init::init(repo_path).await.unwrap();
        spawn::spawn_local(repo_path, "test-session").await.unwrap();

        // Create content and snapshot
        let session_dir = repo_path.join(".vibe/sessions/test-session");
        fs::write(session_dir.join("original.txt"), "original").unwrap();
        snapshot::snapshot(repo_path, "test-session").await.unwrap();

        // Modify content
        fs::write(session_dir.join("original.txt"), "modified").unwrap();
        fs::write(session_dir.join("new.txt"), "new file").unwrap();

        // Find the snapshot name
        let snapshots: Vec<_> = fs::read_dir(repo_path.join(".vibe/sessions"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("test-session_snapshot_")
                    && !e.file_name().to_string_lossy().contains("pre-restore")
            })
            .collect();

        let snapshot_name = snapshots[0]
            .file_name()
            .to_string_lossy()
            .strip_prefix("test-session_snapshot_")
            .unwrap()
            .to_string();

        // Restore
        restore(repo_path, "test-session", &snapshot_name, false)
            .await
            .unwrap();

        // Verify backup was created
        let backups: Vec<_> = fs::read_dir(repo_path.join(".vibe/sessions"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("pre-restore")
            })
            .collect();

        assert!(!backups.is_empty(), "Backup should be created");

        // Verify original content was restored
        let content = fs::read_to_string(session_dir.join("original.txt")).unwrap();
        assert_eq!(content, "original");

        // Verify new file was removed
        assert!(!session_dir.join("new.txt").exists());
    }

    #[tokio::test]
    async fn test_restore_no_backup() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        init::init(repo_path).await.unwrap();
        spawn::spawn_local(repo_path, "test-session").await.unwrap();

        let session_dir = repo_path.join(".vibe/sessions/test-session");
        fs::write(session_dir.join("file.txt"), "content").unwrap();
        snapshot::snapshot(repo_path, "test-session").await.unwrap();

        let snapshots: Vec<_> = fs::read_dir(repo_path.join(".vibe/sessions"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("test-session_snapshot_")
            })
            .collect();

        let snapshot_name = snapshots[0]
            .file_name()
            .to_string_lossy()
            .strip_prefix("test-session_snapshot_")
            .unwrap()
            .to_string();

        // Restore with --no-backup
        restore(repo_path, "test-session", &snapshot_name, true)
            .await
            .unwrap();

        // Verify no backup was created
        let backups: Vec<_> = fs::read_dir(repo_path.join(".vibe/sessions"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("pre-restore")
            })
            .collect();

        assert!(backups.is_empty(), "No backup should be created with --no-backup");
    }
}
