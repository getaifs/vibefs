//! `vibe rebase <session>` command - Update session base to current HEAD

use anyhow::{Context, Result};
use std::path::Path;

use crate::commands::spawn::SpawnInfo;
use crate::git::GitRepo;

/// Rebase a session to the current HEAD
///
/// This updates the session's spawn_commit to the current HEAD, effectively
/// moving the base forward. The session's delta files are preserved.
///
/// Note: This is a simple rebase that doesn't check for conflicts between
/// the session deltas and changes in HEAD..spawn_commit. For safety, we
/// warn but allow the user to proceed.
pub async fn rebase<P: AsRef<Path>>(repo_path: P, session: &str, force: bool) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    // Load current session info
    let mut spawn_info = SpawnInfo::load(repo_path, session)
        .with_context(|| format!("Session '{}' not found", session))?;

    // Get current HEAD
    let git = GitRepo::open(repo_path)?;
    let head_commit = git.head_commit()
        .context("Failed to get HEAD commit")?;

    // Check if already at HEAD
    if spawn_info.spawn_commit.as_ref() == Some(&head_commit) {
        println!("Session '{}' is already at HEAD ({})", session, &head_commit[..7]);
        return Ok(());
    }

    let old_base = spawn_info.spawn_commit.clone().unwrap_or_else(|| "unknown".to_string());

    // Show what we're doing
    println!("Rebasing session '{}' to HEAD", session);
    println!("  Old base: {}", &old_base[..12.min(old_base.len())]);
    println!("  New base: {}", &head_commit[..12.min(head_commit.len())]);

    // Check for potential conflicts by looking at what files changed in HEAD
    let session_dir = vibe_dir.join("sessions").join(session);
    let session_files = list_session_files(&session_dir)?;

    if !session_files.is_empty() {
        // Get files changed between old base and HEAD
        let changed_in_git = get_changed_files(&git, &old_base, &head_commit)?;

        // Find conflicts
        let conflicts: Vec<_> = session_files.iter()
            .filter(|f| changed_in_git.contains(*f))
            .collect();

        if !conflicts.is_empty() {
            println!("\n⚠ WARNING: The following files were modified in both the session and Git:");
            for file in &conflicts {
                println!("  - {}", file);
            }
            println!("\nRebasing will keep your session changes, but you may need to manually");
            println!("reconcile with the Git changes when you promote.");

            if !force {
                println!("\nUse 'vibe rebase {} --force' to proceed anyway.", session);
                return Ok(());
            }
            println!("\nProceeding with --force...");
        }
    }

    // Update spawn_commit
    spawn_info.spawn_commit = Some(head_commit.clone());

    // Save updated spawn info
    let info_path = vibe_dir.join("sessions").join(format!("{}.json", session));
    let info_json = serde_json::to_string_pretty(&spawn_info)?;
    std::fs::write(&info_path, info_json)?;

    println!("\n✓ Session '{}' rebased to {}", session, &head_commit[..7]);
    println!("\nNote: The NFS mount still serves the old Git tree until you restart the session.");
    println!("Run 'vibe close {} && vibe spawn {}' to fully refresh.", session, session);

    Ok(())
}

/// List files in the session delta directory
fn list_session_files(session_dir: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();

    if !session_dir.exists() {
        return Ok(files);
    }

    fn walk_dir(dir: &Path, base: &Path, files: &mut Vec<String>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                walk_dir(&path, base, files)?;
            } else {
                if let Ok(rel_path) = path.strip_prefix(base) {
                    let rel_str = rel_path.to_string_lossy().to_string();
                    // Skip macOS metadata files
                    if !rel_str.starts_with("._") && !rel_str.ends_with(".DS_Store") {
                        files.push(rel_str);
                    }
                }
            }
        }
        Ok(())
    }

    walk_dir(session_dir, session_dir, &mut files)?;
    Ok(files)
}

/// Get files changed between two commits
fn get_changed_files(git: &GitRepo, from: &str, to: &str) -> Result<Vec<String>> {
    use std::process::Command;

    let output = Command::new("git")
        .args(["diff", "--name-only", from, to])
        .current_dir(git.repo_path())
        .output()
        .context("Failed to run git diff")?;

    if !output.status.success() {
        // If git diff fails (e.g., invalid commit), return empty list
        return Ok(Vec::new());
    }

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        use std::fs;
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

    #[test]
    fn test_list_session_files_empty() {
        let temp_dir = TempDir::new().unwrap();
        let files = list_session_files(temp_dir.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_list_session_files_with_content() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "content").unwrap();
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        std::fs::write(temp_dir.path().join("subdir/file2.txt"), "content").unwrap();

        let files = list_session_files(temp_dir.path()).unwrap();
        assert!(files.contains(&"file1.txt".to_string()));
        assert!(files.contains(&"subdir/file2.txt".to_string()));
    }
}
