use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::db::{InodeMetadata, MetadataStore};
use crate::git::GitRepo;

/// Initialize VibeFS for a Git repository
pub async fn init<P: AsRef<Path>>(repo_path: P) -> Result<()> {
    let repo_path = repo_path.as_ref();

    println!("Initializing VibeFS for repository at: {}", repo_path.display());

    // Open the Git repository
    let git = GitRepo::open(repo_path)
        .context("Failed to open Git repository")?;

    // Create .vibe directory structure
    let vibe_dir = repo_path.join(".vibe");
    std::fs::create_dir_all(&vibe_dir)
        .context("Failed to create .vibe directory")?;

    let sessions_dir = vibe_dir.join("sessions");
    std::fs::create_dir_all(&sessions_dir)
        .context("Failed to create sessions directory")?;

    let cache_dir = vibe_dir.join("cache");
    std::fs::create_dir_all(&cache_dir)
        .context("Failed to create cache directory")?;

    // Open/create metadata store
    let metadata_path = vibe_dir.join("metadata.db");
    let metadata = MetadataStore::open(&metadata_path)
        .context("Failed to create metadata store")?;

    println!("Scanning Git repository...");

    // Get HEAD commit
    let head_oid = git.head_commit()
        .context("Failed to get HEAD commit")?;

    // List all files in the tree
    let entries = git.list_tree_files()
        .context("Failed to list tree files")?;

    println!("Found {} entries", entries.len());

    // Create root inode
    let root_metadata = InodeMetadata {
        path: "".to_string(),
        git_oid: Some(head_oid),
        is_dir: true,
        size: 0,
        volatile: false,
    };
    metadata.put_inode(1, &root_metadata)?;

    // Initialize the inode counter to start at 2 (since 1 is used for root)
    // This prevents next_inode_id() from returning 1 and overwriting root
    let _ = metadata.next_inode_id()?; // This sets the counter to 1 and returns 1, which we discard

    // Populate metadata for all entries
    for (path, oid) in entries {
        let inode_id = metadata.next_inode_id()?;

        let size = git.read_blob(&oid)
            .map(|data| data.len() as u64)
            .unwrap_or(0);

        let inode_metadata = InodeMetadata {
            path: path.to_string_lossy().to_string(),
            git_oid: Some(oid),
            is_dir: false,  // All entries from ls-tree -r are files
            size,
            volatile: false,
        };

        metadata.put_inode(inode_id, &inode_metadata)?;
    }

    println!("✓ VibeFS initialized successfully");
    println!("  Metadata store: {}", metadata_path.display());
    println!("  Sessions dir: {}", sessions_dir.display());
    println!("  Cache dir: {}", cache_dir.display());

    // Explicitly drop metadata to ensure RocksDB flushes
    drop(metadata);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        fs::create_dir_all(repo_path.join("src")).unwrap();
        fs::write(repo_path.join("README.md"), "# Test").unwrap();
        fs::write(repo_path.join("src/main.rs"), "fn main() {}").unwrap();

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
    async fn test_init() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        init(repo_path).await.unwrap();

        // Verify .vibe directory structure
        assert!(repo_path.join(".vibe").exists());
        assert!(repo_path.join(".vibe/sessions").exists());
        assert!(repo_path.join(".vibe/cache").exists());
        assert!(repo_path.join(".vibe/metadata.db").exists());

        // Verify metadata store
        let metadata = MetadataStore::open(repo_path.join(".vibe/metadata.db")).unwrap();
        let root = metadata.get_inode(1).unwrap().unwrap();
        assert!(root.is_dir);
        assert_eq!(root.path, "");
    }
}
