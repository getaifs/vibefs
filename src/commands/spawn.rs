use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};

use crate::cwd_validation;
use crate::daemon_client::{ensure_daemon_running, DaemonClient};
use crate::daemon_ipc::DaemonResponse;
use crate::git::GitRepo;
use crate::platform;

/// Directories that should be symlinked to local storage for performance
/// and to avoid macOS NFS xattr issues with build tools.
const ARTIFACT_DIRS: &[&str] = &[
    "target",           // Rust/Cargo
    "node_modules",     // Node.js/npm
    ".venv",            // Python virtualenv
    "__pycache__",      // Python cache
    ".next",            // Next.js
    ".nuxt",            // Nuxt.js
    "dist",             // Common build output
    "build",            // Common build output
];

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

    // Capture HEAD commit at spawn time
    let git_repo = GitRepo::open(repo_path)?;
    let spawn_commit = git_repo.head_commit().ok();

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
                spawn_commit: spawn_commit.clone(),
                created_at: Some(Utc::now().to_rfc3339()),
            };

            let info_path = vibe_dir.join("sessions").join(format!("{}.json", vibe_id));
            let info_json = serde_json::to_string_pretty(&spawn_info)?;
            std::fs::write(&info_path, info_json)?;

            // Create symlinks for build artifact directories
            // These point to local storage to avoid NFS xattr issues with build tools
            let session_dir = vibe_dir.join("sessions").join(&vibe_id);
            if let Err(e) = setup_artifact_symlinks(&session_dir, &vibe_id) {
                eprintln!("  Warning: Failed to setup artifact symlinks: {}", e);
                // Non-fatal - continue with spawn
            } else {
                println!("  ✓ Build artifact directories linked to local storage");
            }

            // Try to mount NFS (works automatically on macOS, requires manual step on Linux)
            println!("\n  NFS server running on port {}", nfs_port);
            match platform::mount_nfs(&mount_point, nfs_port) {
                Ok(_) => {
                    println!("  ✓ NFS mounted at: {}", mount_point);
                }
                Err(e) => {
                    // NFS mounting failed - provide instructions but don't fail
                    println!("  ℹ NFS mount requires manual setup:");
                    println!("    {}", e);
                    println!("\n  Or work directly in session directory:");
                    println!("    {}", vibe_dir.join("sessions").join(&vibe_id).display());
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
/// This function handles stale mounts by unmounting first.
/// Now uses the platform-specific implementation.
pub fn mount_nfs(mount_point: &str, port: u16) -> Result<()> {
    platform::mount_nfs(mount_point, port)
}

/// Unmount NFS share
/// Now uses the platform-specific implementation.
pub fn unmount_nfs(mount_point: &str) -> Result<()> {
    platform::unmount_nfs_sync(mount_point)
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SpawnInfo {
    pub vibe_id: String,
    pub session_dir: PathBuf,
    pub mount_point: PathBuf,
    pub port: u16,
    /// The HEAD commit at spawn time (for diff and drift detection)
    #[serde(default)]
    pub spawn_commit: Option<String>,
    /// Timestamp when session was created
    #[serde(default)]
    pub created_at: Option<String>,
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

/// Set up symlinks for build artifact directories.
/// These directories are symlinked to local storage (/tmp/vibe-artifacts/<session>/)
/// to avoid macOS NFS xattr issues and improve build performance.
fn setup_artifact_symlinks(session_dir: &Path, vibe_id: &str) -> Result<()> {
    // Base directory for local artifacts
    let artifacts_base = PathBuf::from("/tmp/vibe-artifacts").join(vibe_id);

    for dir_name in ARTIFACT_DIRS {
        let local_path = artifacts_base.join(dir_name);
        let symlink_path = session_dir.join(dir_name);

        // Skip if symlink already exists
        if symlink_path.exists() || symlink_path.is_symlink() {
            continue;
        }

        // Create the local directory
        std::fs::create_dir_all(&local_path)
            .with_context(|| format!("Failed to create local artifact dir: {}", local_path.display()))?;

        // Create symlink in session directory
        #[cfg(unix)]
        std::os::unix::fs::symlink(&local_path, &symlink_path)
            .with_context(|| format!("Failed to create symlink: {} -> {}", symlink_path.display(), local_path.display()))?;
    }

    Ok(())
}

/// Clean up artifact symlinks and their local directories.
/// Called when closing a session.
pub fn cleanup_artifact_symlinks(vibe_id: &str) -> Result<()> {
    let artifacts_base = PathBuf::from("/tmp/vibe-artifacts").join(vibe_id);

    if artifacts_base.exists() {
        std::fs::remove_dir_all(&artifacts_base)
            .with_context(|| format!("Failed to cleanup artifacts: {}", artifacts_base.display()))?;
    }

    Ok(())
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

    // Capture HEAD commit at spawn time
    let git_repo = GitRepo::open(repo_path)?;
    let spawn_commit = git_repo.head_commit().ok();

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
        spawn_commit,
        created_at: Some(Utc::now().to_rfc3339()),
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
            spawn_commit: Some("abc123def456".to_string()),
            created_at: Some("2026-01-13T10:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: SpawnInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.vibe_id, "test-vibe");
        assert_eq!(parsed.port, 12345);
        assert_eq!(parsed.spawn_commit, Some("abc123def456".to_string()));
        assert!(parsed.created_at.is_some());
    }

    #[tokio::test]
    async fn test_spawn_info_backward_compatible() {
        // Old JSON without spawn_commit should still parse
        let old_json = r#"{
            "vibe_id": "old-session",
            "session_dir": "/tmp/session",
            "mount_point": "/tmp/mount",
            "port": 9999
        }"#;

        let parsed: SpawnInfo = serde_json::from_str(old_json).unwrap();
        assert_eq!(parsed.vibe_id, "old-session");
        assert_eq!(parsed.spawn_commit, None);
        assert_eq!(parsed.created_at, None);
    }
}
