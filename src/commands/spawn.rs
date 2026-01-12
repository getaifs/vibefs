use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db::MetadataStore;
use crate::git::GitRepo;
// use crate::nfs::VibeNFS;

/// Spawn a new vibe workspace
pub async fn spawn<P: AsRef<Path>>(repo_path: P, vibe_id: &str) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    println!("Spawning vibe workspace: {}", vibe_id);

    // Verify VibeFS is initialized
    if !vibe_dir.exists() {
        anyhow::bail!("VibeFS not initialized. Run 'vibe init' first.");
    }

    // Create session directory
    let session_dir = vibe_dir.join("sessions").join(vibe_id);
    std::fs::create_dir_all(&session_dir)
        .context("Failed to create session directory")?;

    // Open metadata store
    let metadata_path = vibe_dir.join("metadata.db");
    let metadata = MetadataStore::open(&metadata_path)
        .context("Failed to open metadata store")?;
    let _metadata = Arc::new(RwLock::new(metadata));

    // Open Git repository
    let git = GitRepo::open(repo_path)
        .context("Failed to open Git repository")?;
    let _git = Arc::new(RwLock::new(git));

    // Create mount point
    let mount_point = PathBuf::from("/tmp/vibe").join(vibe_id);
    std::fs::create_dir_all(&mount_point)
        .context("Failed to create mount point")?;

    println!("  Session directory: {}", session_dir.display());
    println!("  Mount point: {}", mount_point.display());

    // // Create NFS filesystem
    // let _vfs = VibeNFS::new(
    //     metadata.clone(),
    //     git.clone(),
    //     session_dir.clone(),
    //     vibe_id.to_string(),
    // );

    // // Start NFS server
    // println!("Starting NFS server...");

    // // Find an available port
    // let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    // let port = listener.local_addr()?.port();
    // drop(listener);

    let port = 0;  // Placeholder
    println!("  NFS server: Not yet implemented");

    // Store spawn info
    let spawn_info = SpawnInfo {
        vibe_id: vibe_id.to_string(),
        session_dir: session_dir.clone(),
        mount_point: mount_point.clone(),
        port,
    };

    let info_path = vibe_dir.join("sessions").join(format!("{}.json", vibe_id));
    let info_json = serde_json::to_string_pretty(&spawn_info)?;
    std::fs::write(&info_path, info_json)?;

    println!("✓ Vibe workspace spawned successfully");
    println!("\nTo mount the NFS share, run:");
    println!("  sudo mount -t nfs -o port={},mountport={},nolocks 127.0.0.1:/ {}",
             port, port, mount_point.display());

    // Note: In a real implementation, we would start the NFS server here
    // using nfsserve library and keep it running in the background.
    // For now, we just set up the infrastructure.

    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SpawnInfo {
    vibe_id: String,
    session_dir: PathBuf,
    mount_point: PathBuf,
    port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::init;
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
    async fn test_spawn() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Initialize VibeFS
        init::init(repo_path).await.unwrap();

        // Spawn a vibe
        spawn(repo_path, "test-vibe").await.unwrap();

        // Verify session directory
        assert!(repo_path.join(".vibe/sessions/test-vibe").exists());

        // Verify spawn info file
        let info_path = repo_path.join(".vibe/sessions/test-vibe.json");
        assert!(info_path.exists());

        let info_json = fs::read_to_string(&info_path).unwrap();
        let info: SpawnInfo = serde_json::from_str(&info_json).unwrap();
        assert_eq!(info.vibe_id, "test-vibe");
    }
}
