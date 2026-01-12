use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cwd_validation;
use crate::daemon_client::{ensure_daemon_running, DaemonClient};
use crate::daemon_ipc::DaemonResponse;

/// Spawn a new vibe workspace
pub async fn spawn<P: AsRef<Path>>(repo_path: P, vibe_id: &str) -> Result<()> {
    // Validate that we're running from the correct directory
    let _validated_root = cwd_validation::validate_cwd().context("Cannot spawn vibe workspace")?;

    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    println!("Spawning vibe workspace: {}", vibe_id);

    // Verify VibeFS is initialized
    if !vibe_dir.exists() {
        anyhow::bail!("VibeFS not initialized. Run 'vibe init' first.");
    }

    // Ensure daemon is running
    println!("  Ensuring daemon is running...");
    ensure_daemon_running(repo_path).await?;

    // Connect to daemon and export session
    let mut client = DaemonClient::connect(repo_path).await?;

    match client.export_session(vibe_id).await? {
        DaemonResponse::SessionExported {
            vibe_id,
            nfs_port,
            mount_point,
        } => {
            println!("  Session directory: {}", vibe_dir.join("sessions").join(&vibe_id).display());
            println!("  NFS port: {}", nfs_port);
            println!("  Mount point: {}", mount_point);

            // Save spawn info for other commands
            let spawn_info = SpawnInfo {
                vibe_id: vibe_id.clone(),
                session_dir: vibe_dir.join("sessions").join(&vibe_id),
                mount_point: PathBuf::from(&mount_point),
                port: nfs_port,
            };

            let info_path = vibe_dir.join("sessions").join(format!("{}.json", vibe_id));
            let info_json = serde_json::to_string_pretty(&spawn_info)?;
            std::fs::write(&info_path, info_json)?;

            // Attempt to mount using sudo-less NFS mount
            println!("\n  Attempting NFS mount...");
            match mount_nfs(&mount_point, nfs_port) {
                Ok(_) => {
                    println!("✓ Vibe workspace mounted at: {}", mount_point);
                }
                Err(e) => {
                    println!("⚠ Auto-mount failed: {}", e);
                    println!("\n  To mount manually, run:");
                    println!(
                        "  mount_nfs -o vers=3,tcp,port={},mountport={},noresvport,nolock,locallocks localhost:/ {}",
                        nfs_port, nfs_port, mount_point
                    );
                }
            }

            println!("\n✓ Vibe workspace spawned successfully");
        }
        DaemonResponse::Error { message } => {
            anyhow::bail!("Daemon error: {}", message);
        }
        other => {
            anyhow::bail!("Unexpected daemon response: {:?}", other);
        }
    }

    Ok(())
}

/// Mount NFS share without sudo (using high port and user-space mount)
fn mount_nfs(mount_point: &str, port: u16) -> Result<()> {
    // Create mount point if it doesn't exist
    std::fs::create_dir_all(mount_point)?;

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
            &format!("localhost:/"),
            mount_point,
        ])
        .output()
        .context("Failed to execute mount_nfs")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("mount_nfs failed: {}", stderr);
    }

    Ok(())
}

/// Unmount NFS share
pub fn unmount_nfs(mount_point: &str) -> Result<()> {
    let output = Command::new("umount")
        .arg(mount_point)
        .output()
        .context("Failed to execute umount")?;

    if !output.status.success() {
        // Try force unmount
        let output = Command::new("umount")
            .args(["-f", mount_point])
            .output()
            .context("Failed to execute umount -f")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("umount failed: {}", stderr);
        }
    }

    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SpawnInfo {
    pub vibe_id: String,
    pub session_dir: PathBuf,
    pub mount_point: PathBuf,
    pub port: u16,
}

impl SpawnInfo {
    /// Load spawn info for a vibe
    pub fn load(repo_path: &Path, vibe_id: &str) -> Result<Self> {
        let info_path = repo_path
            .join(".vibe/sessions")
            .join(format!("{}.json", vibe_id));
        let json = std::fs::read_to_string(&info_path)
            .with_context(|| format!("Vibe '{}' not found", vibe_id))?;
        let info: SpawnInfo = serde_json::from_str(&json)?;
        Ok(info)
    }
}

/// Local spawn without daemon (for testing and simple use cases)
/// This creates the session directory structure without NFS mounting
pub async fn spawn_local<P: AsRef<Path>>(repo_path: P, vibe_id: &str) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    // Verify VibeFS is initialized
    if !vibe_dir.exists() {
        anyhow::bail!("VibeFS not initialized. Run 'vibe init' first.");
    }

    // Create session directory
    let session_dir = vibe_dir.join("sessions").join(vibe_id);
    std::fs::create_dir_all(&session_dir)
        .context("Failed to create session directory")?;

    // Create mount point (for compatibility)
    let mount_point = PathBuf::from("/tmp/vibe").join(vibe_id);
    std::fs::create_dir_all(&mount_point)
        .context("Failed to create mount point")?;

    // Store spawn info
    let spawn_info = SpawnInfo {
        vibe_id: vibe_id.to_string(),
        session_dir: session_dir.clone(),
        mount_point: mount_point.clone(),
        port: 0,
    };

    let info_path = vibe_dir.join("sessions").join(format!("{}.json", vibe_id));
    let info_json = serde_json::to_string_pretty(&spawn_info)?;
    std::fs::write(&info_path, info_json)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn setup_test_repo() -> tempfile::TempDir {
        use std::fs;
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize a new git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Configure user
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

        // Create initial commit
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
    async fn test_spawn_info_serialization() {
        let info = SpawnInfo {
            vibe_id: "test-vibe".to_string(),
            session_dir: PathBuf::from("/tmp/session"),
            mount_point: PathBuf::from("/tmp/mount"),
            port: 12345,
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: SpawnInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.vibe_id, "test-vibe");
        assert_eq!(parsed.port, 12345);
    }
}
