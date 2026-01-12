use anyhow::{Context, Result};
use std::path::Path;

use crate::git::GitRepo;

/// Finalize a vibe into main history
pub async fn commit<P: AsRef<Path>>(repo_path: P, vibe_id: &str) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");
    let session_dir = vibe_dir.join("sessions").join(vibe_id);

    if !session_dir.exists() {
        anyhow::bail!("Vibe session '{}' does not exist", vibe_id);
    }

    println!("Committing vibe session: {}", vibe_id);

    // Open Git repository
    let git = GitRepo::open(repo_path)
        .context("Failed to open Git repository")?;

    // Get the promoted commit from refs/vibes/<vibe_id>
    let ref_name = format!("refs/vibes/{}", vibe_id);
    let vibe_commit_oid = git.get_ref(&ref_name)
        .context("Failed to get vibe reference")?
        .ok_or_else(|| anyhow::anyhow!(
            "Vibe session '{}' has not been promoted. Run 'vibe promote {}' first.",
            vibe_id, vibe_id
        ))?;

    println!("  Vibe commit: {}", vibe_commit_oid);

    // Get current HEAD
    let head_oid = git.head_commit()
        .context("Failed to get HEAD commit")?;

    if head_oid == vibe_commit_oid {
        println!("Already at vibe commit, nothing to do");
        return Ok(());
    }

    // Update HEAD to point to the vibe commit
    println!("Updating HEAD...");
    git.update_ref("HEAD", &vibe_commit_oid)
        .context("Failed to update HEAD")?;

    // Update working tree to match new HEAD
    let output = std::process::Command::new("git")
        .args(&["reset", "--hard", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to reset working tree")?;

    if !output.status.success() {
        eprintln!("Warning: Failed to update working tree");
    }

    println!("✓ Vibe session committed successfully");
    println!("  Previous HEAD: {}", head_oid);
    println!("  New HEAD: {}", vibe_commit_oid);

    // Clean up session directory
    println!("Cleaning up session directory...");
    std::fs::remove_dir_all(&session_dir)
        .context("Failed to remove session directory")?;

    // Remove spawn info
    let info_path = vibe_dir.join("sessions").join(format!("{}.json", vibe_id));
    if info_path.exists() {
        std::fs::remove_file(&info_path)
            .context("Failed to remove spawn info")?;
    }

    println!("✓ Session cleanup complete");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{init, promote, spawn};
    use crate::db::MetadataStore;
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
    async fn test_commit() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Initialize and spawn
        init::init(repo_path).await.unwrap();
        spawn::spawn(repo_path, "test-vibe").await.unwrap();

        // Modify a file in the session
        let session_dir = repo_path.join(".vibe/sessions/test-vibe");
        fs::write(session_dir.join("new_file.txt"), "new content").unwrap();

        // Mark as dirty
        let metadata_path = repo_path.join(".vibe/metadata.db");
        let metadata = MetadataStore::open(&metadata_path).unwrap();
        metadata.mark_dirty("new_file.txt").unwrap();

        // Promote
        promote::promote(repo_path, "test-vibe").await.unwrap();

        // Get vibe commit OID before committing
        let git = GitRepo::open(repo_path).unwrap();
        let vibe_oid = git.get_ref("refs/vibes/test-vibe").unwrap().unwrap();

        // Commit
        commit(repo_path, "test-vibe").await.unwrap();

        // Verify HEAD was updated
        let new_head = git.head_commit().unwrap();
        assert_eq!(new_head, vibe_oid);

        // Verify session directory was cleaned up
        assert!(!session_dir.exists());
    }
}
