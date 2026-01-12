use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::db::MetadataStore;
use crate::git::GitRepo;

/// Promote a vibe session into a Git commit
pub async fn promote<P: AsRef<Path>>(repo_path: P, vibe_id: &str) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");
    let session_dir = vibe_dir.join("sessions").join(vibe_id);

    if !session_dir.exists() {
        anyhow::bail!("Vibe session '{}' does not exist", vibe_id);
    }

    println!("Promoting vibe session: {}", vibe_id);

    // Open metadata store
    let metadata_path = vibe_dir.join("metadata.db");
    let metadata = MetadataStore::open(&metadata_path)
        .context("Failed to open metadata store")?;

    // Open Git repository
    let git = GitRepo::open(repo_path)
        .context("Failed to open Git repository")?;

    // Get dirty paths (modified files)
    let dirty_paths = metadata.get_dirty_paths()
        .context("Failed to get dirty paths")?;

    if dirty_paths.is_empty() {
        println!("No changes to promote");
        return Ok(());
    }

    println!("Found {} modified files:", dirty_paths.len());
    for path in &dirty_paths {
        println!("  - {}", path);
    }

    // Hash new blobs for modified files
    let mut new_blobs = HashMap::new();
    for path in &dirty_paths {
        let file_path = session_dir.join(path);
        if file_path.exists() && file_path.is_file() {
            let content = std::fs::read(&file_path)
                .with_context(|| format!("Failed to read {}", path))?;

            let oid = git.write_blob(&content)
                .with_context(|| format!("Failed to hash blob for {}", path))?;

            println!("  Hashed {} -> {}", path, &oid);
            new_blobs.insert(path.clone(), oid);
        }
    }

    // Build new tree by copying modified files into git index
    println!("Building new Git commit...");

    let head_oid = git.head_commit()
        .context("Failed to get HEAD commit")?;

    // Create a temporary index based on HEAD
    let temp_index = session_dir.parent().unwrap().join(format!("{}_index", vibe_id));

    // Read HEAD tree into temporary index
    let output = Command::new("git")
        .args(&["read-tree", &head_oid])
        .env("GIT_INDEX_FILE", &temp_index)
        .current_dir(repo_path)
        .output()
        .context("Failed to read HEAD tree")?;

    if !output.status.success() {
        anyhow::bail!("Failed to read tree");
    }

    // Update index with modified files
    for (path, oid) in &new_blobs {
        let output = Command::new("git")
            .args(&["update-index", "--add", "--cacheinfo", &format!("100644,{},{}", oid, path)])
            .env("GIT_INDEX_FILE", &temp_index)
            .current_dir(repo_path)
            .output()
            .context("Failed to update index")?;

        if !output.status.success() {
            eprintln!("Warning: Failed to update index for {}", path);
        }
    }

    // Write tree from index
    let output = Command::new("git")
        .args(&["write-tree"])
        .env("GIT_INDEX_FILE", &temp_index)
        .current_dir(repo_path)
        .output()
        .context("Failed to write tree")?;

    if !output.status.success() {
        anyhow::bail!("Failed to write tree");
    }

    let tree_oid = String::from_utf8(output.stdout)?.trim().to_string();
    println!("  Created tree: {}", tree_oid);

    // Clean up temporary index
    let _ = std::fs::remove_file(&temp_index);

    // Create commit with HEAD as parent
    let commit_message = format!("Vibe promotion: {}\n\nPromoted changes from vibe session", vibe_id);

    let commit_oid = git.create_commit(&tree_oid, &head_oid, &commit_message)
        .context("Failed to create commit")?;
    println!("  Created commit: {}", commit_oid);

    // Update refs/vibes/<vibe_id> reference
    let ref_name = format!("refs/vibes/{}", vibe_id);
    git.update_ref(&ref_name, &commit_oid)
        .context("Failed to update reference")?;

    println!("✓ Vibe session promoted successfully");
    println!("  Reference: {}", ref_name);
    println!("  Commit: {}", commit_oid);
    println!("\nTo merge into main, run: vibe commit {}", vibe_id);

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
    async fn test_promote() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Initialize and spawn
        init::init(repo_path).await.unwrap();
        spawn::spawn(repo_path, "test-vibe").await.unwrap();

        // Modify a file in the session
        let session_dir = repo_path.join(".vibe/sessions/test-vibe");
        fs::write(session_dir.join("new_file.txt"), "new content").unwrap();

        // Mark as dirty
        {
            let metadata_path = repo_path.join(".vibe/metadata.db");
            let metadata = MetadataStore::open(&metadata_path).unwrap();
            metadata.mark_dirty("new_file.txt").unwrap();
        } // metadata is dropped here, releasing the lock

        // Promote
        promote(repo_path, "test-vibe").await.unwrap();

        // Verify reference was created
        let git = GitRepo::open(repo_path).unwrap();
        let ref_oid = git.get_ref("refs/vibes/test-vibe").unwrap();
        assert!(ref_oid.is_some());
    }
}
