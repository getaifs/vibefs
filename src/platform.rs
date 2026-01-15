//! Platform-specific operations for VibeFS
//! Handles differences between macOS and Linux for mount points, NFS, and reflinks.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Get the mount point directory for VibeFS based on the platform
pub fn get_vibe_mounts_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from(format!(
            "{}/Library/Caches/vibe/mounts",
            std::env::var("HOME").unwrap_or_default()
        ))
    }

    #[cfg(target_os = "linux")]
    {
        PathBuf::from(format!(
            "{}/.cache/vibe/mounts",
            std::env::var("HOME").unwrap_or_default()
        ))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        PathBuf::from("/tmp/vibe/mounts")
    }
}

/// Mount an NFS share at the specified mount point and port
/// Handles platform-specific mount command differences
pub fn mount_nfs(mount_point: &str, port: u16) -> Result<()> {
    // Create mount point if it doesn't exist
    std::fs::create_dir_all(mount_point)?;

    // Check if already mounted - unmount stale mounts first
    let mount_output = Command::new("mount")
        .output()
        .context("Failed to check mounts")?;

    let mount_list = String::from_utf8_lossy(&mount_output.stdout);
    let is_mounted = mount_list.lines().any(|line| line.contains(mount_point));

    if is_mounted {
        // Try to unmount existing (possibly stale) mount
        unmount_nfs_sync(mount_point).ok();
        // Give it a moment
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    #[cfg(target_os = "macos")]
    {
        // macOS mount_nfs options for user-space mounting
        // -o noresvport: Use non-reserved ports (allows non-root mount on macOS)
        // -o vers=3: Use NFSv3 (nfsserve is v3)
        // -o tcp: Use TCP transport
        // -o port=<port>: Connect to specified port
        // -o mountport=<port>: Use same port for MOUNT protocol (nfsserve multiplexes)
        // -o nolock,locallocks: Disable NFS locking (we handle it ourselves)
        let output = Command::new("mount_nfs")
            .args([
                "-o",
                &format!(
                    "vers=3,tcp,port={},mountport={},noresvport,nolock,locallocks,noacl,soft,retrans=2,timeo=5",
                    port, port
                ),
                "localhost:/",
                mount_point,
            ])
            .output()
            .context("Failed to execute mount_nfs")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("mount_nfs failed: {}", stderr);
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Linux NFS mounting requires root privileges
        // We just provide the command for the user to run manually
        // This allows the project to work universally without assuming sudo access

        anyhow::bail!(
            "NFS mounting on Linux requires root privileges.\n\
             Please run manually:\n\
             sudo mount -t nfs -o vers=3,tcp,port={},mountport={},nolock localhost:/ {}",
            port, port, mount_point
        );
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!("NFS mounting not supported on this platform");
    }

    #[cfg(target_os = "macos")]
    Ok(())
}

/// Unmount an NFS share (synchronous version)
pub fn unmount_nfs_sync(mount_point: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        // Try diskutil first
        let output = Command::new("diskutil")
            .args(["unmount", "force", mount_point])
            .output();

        if output.is_ok() && output.unwrap().status.success() {
            return Ok(());
        }

        // Fallback to umount -f
        let output = Command::new("umount")
            .args(["-f", mount_point])
            .output()
            .context("Failed to execute umount")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("umount failed: {}", stderr);
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Linux unmounting requires root privileges
        // Provide helpful error message
        anyhow::bail!(
            "NFS unmounting on Linux requires root privileges.\n\
             Please run manually:\n\
             sudo umount {}",
            mount_point
        );
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!("NFS unmounting not supported on this platform");
    }

    #[cfg(target_os = "macos")]
    Ok(())
}

/// Detect if the current or given path is inside a vibe mount
/// Returns the original repo path if found
pub fn detect_vibe_mount_origin(start_path: &Path) -> Option<PathBuf> {
    let mounts_dir = get_vibe_mounts_dir();

    // Check if we're inside the mounts directory
    if !start_path.starts_with(&mounts_dir) {
        return None;
    }

    // Walk up from start_path looking for .vibe-origin file
    let mut current = start_path.to_path_buf();
    loop {
        let origin_file = current.join(".vibe-origin");
        if origin_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&origin_file) {
                let repo_path = PathBuf::from(content.trim());
                if repo_path.exists() {
                    return Some(repo_path);
                }
            }
        }

        // Stop if we've reached the mounts directory or can't go up further
        if current == mounts_dir || !current.pop() {
            break;
        }
    }

    None
}

/// Get the effective repo path, detecting if we're in a vibe mount
pub fn get_effective_repo_path(specified_path: &Path) -> PathBuf {
    // First, try to canonicalize the specified path
    let canonical = specified_path.canonicalize().unwrap_or_else(|_| specified_path.to_path_buf());

    // Check if we're in a vibe mount
    if let Some(origin) = detect_vibe_mount_origin(&canonical) {
        return origin;
    }

    // Also check current directory if it's different
    if let Ok(cwd) = std::env::current_dir() {
        if cwd != canonical {
            if let Some(origin) = detect_vibe_mount_origin(&cwd) {
                return origin;
            }
        }
    }

    canonical
}
