use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::cwd_validation;

/// Create a zero-cost snapshot of a vibe session
pub async fn snapshot<P: AsRef<Path>>(repo_path: P, vibe_id: &str) -> Result<()> {
    // Validate that we're running from the correct directory
    let _validated_root = cwd_validation::validate_cwd()
        .context("Cannot create snapshot")?;

    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");
    let session_dir = vibe_dir.join("sessions").join(vibe_id);

    if !session_dir.exists() {
        anyhow::bail!("Vibe session '{}' does not exist", vibe_id);
    }

    // Create snapshot directory
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let snapshot_name = format!("{}_snapshot_{}", vibe_id, timestamp);
    let snapshot_dir = vibe_dir.join("sessions").join(&snapshot_name);

    println!("Creating snapshot: {}", snapshot_name);
    println!("  Source: {}", session_dir.display());
    println!("  Destination: {}", snapshot_dir.display());

    // Use platform-specific CoW copy
    #[cfg(target_os = "macos")]
    {
        copy_with_clonefile(&session_dir, &snapshot_dir)?;
    }

    #[cfg(target_os = "linux")]
    {
        copy_with_reflink(&session_dir, &snapshot_dir)?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        // Fallback to regular copy
        copy_recursive(&session_dir, &snapshot_dir)?;
    }

    println!("✓ Snapshot created successfully: {}", snapshot_name);

    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_with_clonefile(src: &Path, dst: &Path) -> Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let src_cstr = CString::new(src.as_os_str().as_bytes())?;
    let dst_cstr = CString::new(dst.as_os_str().as_bytes())?;

    // Use clonefile(2) for APFS CoW copy
    let result = unsafe {
        libc::clonefile(
            src_cstr.as_ptr(),
            dst_cstr.as_ptr(),
            0, // flags
        )
    };

    if result != 0 {
        anyhow::bail!("clonefile failed: {}", std::io::Error::last_os_error());
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn copy_with_reflink(src: &Path, dst: &Path) -> Result<()> {
    // Try cp with --reflink=always for CoW copy on Btrfs/XFS
    let output = Command::new("cp")
        .arg("-r")
        .arg("--reflink=always")
        .arg(src)
        .arg(dst)
        .output()
        .context("Failed to execute cp with reflink")?;

    if !output.status.success() {
        // Reflink not supported, fall back to regular copy
        eprintln!("Warning: reflink not supported on this filesystem, using regular copy");
        copy_recursive(src, dst)?;
    }

    Ok(())
}

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
    use crate::commands::{init, spawn};
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize a new git repo
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Configure user
        std::process::Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create initial commit
        fs::write(repo_path.join("README.md"), "# Test").unwrap();

        std::process::Command::new("git")
            .args(&["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(&["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        temp_dir
    }

    #[tokio::test]
    async fn test_snapshot() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Initialize and spawn
        init::init(repo_path).await.unwrap();
        spawn::spawn(repo_path, "test-vibe").await.unwrap();

        // Create a test file in the session
        let session_dir = repo_path.join(".vibe/sessions/test-vibe");
        fs::write(session_dir.join("test.txt"), "test content").unwrap();

        // Create snapshot
        snapshot(repo_path, "test-vibe").await.unwrap();

        // Verify snapshot exists
        let snapshots: Vec<_> = fs::read_dir(repo_path.join(".vibe/sessions"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("test-vibe_snapshot_")
            })
            .collect();

        assert!(!snapshots.is_empty());

        // Verify snapshot contains the test file
        let snapshot_dir = snapshots[0].path();
        assert!(snapshot_dir.join("test.txt").exists());
    }
}
