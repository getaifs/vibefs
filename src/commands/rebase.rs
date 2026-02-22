//! `vibe rebase <session>` command - Update session base to current HEAD

use anyhow::{Context, Result};
use std::path::Path;

use crate::commands::spawn::SpawnInfo;
use crate::daemon_client::DaemonClient;
use crate::daemon_ipc::DaemonResponse;
use crate::db::MetadataStore;
use crate::git::GitRepo;
use crate::platform;

/// Check if our cwd is inside the given mount path
fn is_cwd_inside_mount(mount_point: &str) -> bool {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| cwd.to_str().map(|s| s.starts_with(mount_point)))
        .unwrap_or(false)
}

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

    // If daemon is running, try RPC rebase (keeps NFS alive, no bricked shells)
    if DaemonClient::is_running(repo_path).await {
        let rpc_result = async {
            let mut client = DaemonClient::connect(repo_path).await?;
            client.rebase_session(session, force).await
        }
        .await;

        match rpc_result {
            Ok(DaemonResponse::SessionRebased { reconciled_count, .. }) => {
                if reconciled_count > 0 {
                    println!("  Cleaned up {} stale file(s) that match HEAD", reconciled_count);
                }
                return Ok(());
            }
            Ok(DaemonResponse::Error { message }) => {
                eprintln!("Warning: daemon rebase failed: {}. Falling back to legacy path.", message);
            }
            Err(e) => {
                eprintln!("Warning: daemon RPC failed: {}. Falling back to legacy path.", e);
            }
            _ => {
                eprintln!("Warning: unexpected daemon response. Falling back to legacy path.");
            }
        }

        // Legacy fallback: unmount, reconcile, re-export
        print!("  Restarting NFS mount...");

        let mut client = DaemonClient::connect(repo_path).await?;
        match client.unexport_session(session).await? {
            DaemonResponse::SessionUnexported { .. } => {}
            DaemonResponse::Error { message } => {
                eprintln!("\n  Warning: unexport failed: {}", message);
            }
            _ => {}
        }

        let old_mount = spawn_info.mount_point.to_string_lossy().to_string();
        platform::unmount_nfs_sync(&old_mount).ok();

        let session_metadata_db = session_dir.join("metadata.db");
        match reconcile_session_files(&git, &session_dir, &head_commit, Some(&session_metadata_db)) {
            Ok(0) => {}
            Ok(n) => println!("\n  Cleaned up {} stale file(s) that match HEAD", n),
            Err(e) => eprintln!("\n  Warning: reconciliation error: {}", e),
        }

        let mut client = DaemonClient::connect(repo_path).await?;
        match client.export_session(session).await? {
            DaemonResponse::SessionExported { nfs_port, mount_point, .. } => {
                match platform::mount_nfs(&mount_point, nfs_port) {
                    Ok(_) => {
                        println!(" done");
                        println!("  NFS mounted at: {}", mount_point);

                        spawn_info.port = nfs_port;
                        let info_json = serde_json::to_string_pretty(&spawn_info)?;
                        std::fs::write(&info_path, info_json)?;

                        if let Err(e) = platform::register_mount(&mount_point, repo_path) {
                            eprintln!("  Warning: Failed to register mount: {}", e);
                        }

                        if is_cwd_inside_mount(&mount_point) {
                            println!("\n  Your shell's working directory was invalidated by the remount.");
                            println!("  Run: cd {}", mount_point);
                        }
                    }
                    Err(e) => {
                        eprintln!(" mount failed: {}", e);
                        eprintln!("  NFS server running on port {}. Mount manually if needed.", nfs_port);
                    }
                }
            }
            DaemonResponse::Error { message } => {
                eprintln!(" failed: {}", message);
            }
            _ => {}
        }
    } else {
        // Daemon not running — still reconcile stale files
        let session_metadata_db = session_dir.join("metadata.db");
        match reconcile_session_files(&git, &session_dir, &head_commit, Some(&session_metadata_db)) {
            Ok(0) => {}
            Ok(n) => println!("  Cleaned up {} stale file(s) that match HEAD", n),
            Err(e) => eprintln!("  Warning: reconciliation error: {}", e),
        }
        println!("  Note: Daemon not running. Start a session with 'vibe new {}' to apply.", session);
    }

    Ok(())
}

/// Reconcile session files after rebase: remove files that match the new HEAD.
///
/// When a session file is identical to its counterpart in the new HEAD commit,
/// it's a stale copy (not an intentional edit). Removing it lets NFS reads
/// fall through to the updated git tree.
fn reconcile_session_files(
    git: &GitRepo,
    session_dir: &Path,
    head_commit: &str,
    metadata_db_path: Option<&Path>,
) -> Result<usize> {
    let session_files = list_session_files(session_dir)?;
    if session_files.is_empty() {
        return Ok(0);
    }

    let mut reconciled = 0;

    // Try to open per-session metadata.db to clear dirty markers
    let store = metadata_db_path.and_then(|p| {
        if p.exists() {
            MetadataStore::open(p).ok()
        } else {
            None
        }
    });

    for file_path in &session_files {
        let session_file = session_dir.join(file_path);
        if !session_file.exists() || !session_file.is_file() {
            continue;
        }

        // Read session file content
        let session_content = match std::fs::read(&session_file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Read HEAD content for this path
        match git.read_file_at_commit(head_commit, file_path) {
            Ok(Some(head_content)) if head_content == session_content => {
                // Content matches — this is a stale copy, remove it
                if let Err(e) = std::fs::remove_file(&session_file) {
                    eprintln!("  Warning: failed to remove stale file {}: {}", file_path, e);
                    continue;
                }

                // Clean up empty parent directories
                if let Some(parent) = session_file.parent() {
                    let _ = remove_empty_parents(parent, session_dir);
                }

                // Clear dirty marker if we have DB access
                if let Some(ref s) = store {
                    let _ = s.clear_dirty_path(file_path);
                }

                reconciled += 1;
            }
            _ => {
                // File differs from HEAD or doesn't exist in HEAD — keep it
            }
        }
    }

    Ok(reconciled)
}

/// Remove empty parent directories up to (but not including) the base directory
fn remove_empty_parents(dir: &Path, base: &Path) -> Result<()> {
    let mut current = dir;
    while current != base {
        if current.read_dir()?.next().is_none() {
            std::fs::remove_dir(current)?;
        } else {
            break;
        }
        match current.parent() {
            Some(p) => current = p,
            None => break,
        }
    }
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

            // Skip symlinks (artifact directories like target/, node_modules/)
            if path.is_symlink() {
                continue;
            }

            // Skip metadata.db (per-session RocksDB store)
            if let Some(name) = path.file_name() {
                if name == "metadata.db" {
                    continue;
                }
            }

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

    #[test]
    fn test_reconcile_removes_matching_files() {
        use std::fs;

        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create files and commit
        fs::write(repo_path.join("unchanged.txt"), "same content").unwrap();
        fs::write(repo_path.join("modified.txt"), "original").unwrap();
        fs::create_dir_all(repo_path.join("src")).unwrap();
        fs::write(repo_path.join("src/lib.rs"), "pub fn hello() {}").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "add files"]).current_dir(repo_path).output().unwrap();

        let git = GitRepo::open(repo_path).unwrap();
        let head = git.head_commit().unwrap();

        // Simulate session dir with stale + modified files
        let session_dir = temp_dir.path().join("session");
        fs::create_dir_all(session_dir.join("src")).unwrap();

        // This file matches HEAD → should be reconciled (removed)
        fs::write(session_dir.join("unchanged.txt"), "same content").unwrap();
        // This file matches HEAD → should be reconciled
        fs::write(session_dir.join("src/lib.rs"), "pub fn hello() {}").unwrap();
        // This file differs from HEAD → should be kept
        fs::write(session_dir.join("modified.txt"), "changed content").unwrap();
        // This file doesn't exist in HEAD → should be kept
        fs::write(session_dir.join("new_file.txt"), "brand new").unwrap();

        let reconciled = reconcile_session_files(&git, &session_dir, &head, None).unwrap();

        assert_eq!(reconciled, 2, "should reconcile 2 matching files");
        assert!(!session_dir.join("unchanged.txt").exists(), "matching file should be removed");
        assert!(!session_dir.join("src/lib.rs").exists(), "matching nested file should be removed");
        assert!(!session_dir.join("src").exists(), "empty parent dir should be cleaned up");
        assert!(session_dir.join("modified.txt").exists(), "modified file should be kept");
        assert!(session_dir.join("new_file.txt").exists(), "new file should be kept");
    }

    #[test]
    fn test_reconcile_clears_dirty_markers() {
        use std::fs;
        use crate::db::MetadataStore;

        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        fs::write(repo_path.join("file.txt"), "content").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(repo_path).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "add"]).current_dir(repo_path).output().unwrap();

        let git = GitRepo::open(repo_path).unwrap();
        let head = git.head_commit().unwrap();

        // Set up session with matching file and dirty marker
        let session_dir = temp_dir.path().join("session");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(session_dir.join("file.txt"), "content").unwrap();

        let db_path = session_dir.join("metadata.db");
        let store = MetadataStore::open(&db_path).unwrap();
        store.mark_dirty("file.txt").unwrap();
        assert!(store.is_dirty("file.txt").unwrap());
        drop(store);

        let reconciled = reconcile_session_files(&git, &session_dir, &head, Some(&db_path)).unwrap();
        assert_eq!(reconciled, 1);

        // Verify dirty marker was cleared
        let store = MetadataStore::open(&db_path).unwrap();
        assert!(!store.is_dirty("file.txt").unwrap(), "dirty marker should be cleared after reconciliation");
    }
}
