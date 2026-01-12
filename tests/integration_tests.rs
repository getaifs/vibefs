use anyhow::Result;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use vibefs::commands::{commit, init, promote, snapshot, spawn};
use vibefs::db::MetadataStore;
use vibefs::git::GitRepo;

/// Helper to set up a test repository
fn setup_test_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize git repo
    std::process::Command::new("git")
        .args(&["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();

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

    // Create initial files
    fs::create_dir_all(repo_path.join("src")).unwrap();
    fs::write(repo_path.join("README.md"), "# VibeFS Test").unwrap();
    fs::write(repo_path.join("src/main.rs"), "fn main() {\n    println!(\"Hello, world!\");\n}").unwrap();
    fs::write(repo_path.join("src/lib.rs"), "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}").unwrap();

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
async fn test_full_workflow() -> Result<()> {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Test 1: Initialize VibeFS
    init::init(repo_path).await?;

    assert!(repo_path.join(".vibe").exists());
    assert!(repo_path.join(".vibe/sessions").exists());
    assert!(repo_path.join(".vibe/cache").exists());
    assert!(repo_path.join(".vibe/metadata.db").exists());

    // Verify metadata was populated
    let metadata = MetadataStore::open(repo_path.join(".vibe/metadata.db"))?;
    let root = metadata.get_inode(1)?.unwrap();
    assert!(root.is_dir);

    // Test 2: Spawn a vibe workspace
    spawn::spawn(repo_path, "agent-1").await?;

    let session_dir = repo_path.join(".vibe/sessions/agent-1");
    assert!(session_dir.exists());

    // Test 3: Simulate agent modifying files
    fs::write(session_dir.join("new_feature.rs"), "pub fn new_feature() -> bool {\n    true\n}").unwrap();
    fs::write(session_dir.join("README.md"), "# VibeFS Test\n\nUpdated by agent-1").unwrap();

    // Mark files as dirty
    metadata.mark_dirty("new_feature.rs")?;
    metadata.mark_dirty("README.md")?;

    let dirty_paths = metadata.get_dirty_paths()?;
    assert_eq!(dirty_paths.len(), 2);

    // Test 4: Create a snapshot
    snapshot::snapshot(repo_path, "agent-1").await?;

    let snapshots: Vec<_> = fs::read_dir(repo_path.join(".vibe/sessions"))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("agent-1_snapshot_")
        })
        .collect();

    assert!(!snapshots.is_empty());

    // Test 5: Promote the vibe session
    promote::promote(repo_path, "agent-1").await?;

    let git = GitRepo::open(repo_path)?;
    let vibe_ref = git.get_ref("refs/vibes/agent-1")?;
    assert!(vibe_ref.is_some());

    // Test 6: Commit the vibe
    let original_head = git.head_commit()?;
    commit::commit(repo_path, "agent-1").await?;

    // Verify HEAD was updated
    let new_head = git.head_commit()?;
    assert_ne!(original_head, new_head);
    assert_eq!(new_head, vibe_ref.unwrap());

    // Verify session was cleaned up
    assert!(!session_dir.exists());

    Ok(())
}

#[tokio::test]
async fn test_multiple_parallel_vibes() -> Result<()> {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Initialize
    init::init(repo_path).await?;

    // Spawn multiple vibes
    spawn::spawn(repo_path, "agent-1").await?;
    spawn::spawn(repo_path, "agent-2").await?;
    spawn::spawn(repo_path, "agent-3").await?;

    // Verify all sessions exist
    assert!(repo_path.join(".vibe/sessions/agent-1").exists());
    assert!(repo_path.join(".vibe/sessions/agent-2").exists());
    assert!(repo_path.join(".vibe/sessions/agent-3").exists());

    // Simulate each agent working on different files
    let metadata = MetadataStore::open(repo_path.join(".vibe/metadata.db"))?;

    fs::write(
        repo_path.join(".vibe/sessions/agent-1/feature1.rs"),
        "// Feature 1",
    )?;
    metadata.mark_dirty("feature1.rs")?;

    fs::write(
        repo_path.join(".vibe/sessions/agent-2/feature2.rs"),
        "// Feature 2",
    )?;
    metadata.mark_dirty("feature2.rs")?;

    fs::write(
        repo_path.join(".vibe/sessions/agent-3/feature3.rs"),
        "// Feature 3",
    )?;
    metadata.mark_dirty("feature3.rs")?;

    // Promote all vibes
    promote::promote(repo_path, "agent-1").await?;
    promote::promote(repo_path, "agent-2").await?;
    promote::promote(repo_path, "agent-3").await?;

    // Verify all have refs
    let git = GitRepo::open(repo_path)?;
    assert!(git.get_ref("refs/vibes/agent-1")?.is_some());
    assert!(git.get_ref("refs/vibes/agent-2")?.is_some());
    assert!(git.get_ref("refs/vibes/agent-3")?.is_some());

    Ok(())
}

#[tokio::test]
async fn test_snapshot_preserves_state() -> Result<()> {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Initialize and spawn
    init::init(repo_path).await?;
    spawn::spawn(repo_path, "agent-1").await?;

    let session_dir = repo_path.join(".vibe/sessions/agent-1");

    // Create initial state
    fs::write(session_dir.join("file1.txt"), "version 1")?;

    // Create snapshot
    snapshot::snapshot(repo_path, "agent-1").await?;

    // Modify file
    fs::write(session_dir.join("file1.txt"), "version 2")?;

    // Find snapshot
    let snapshots: Vec<_> = fs::read_dir(repo_path.join(".vibe/sessions"))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("agent-1_snapshot_")
        })
        .collect();

    assert_eq!(snapshots.len(), 1);

    // Verify snapshot has old version
    let snapshot_file = snapshots[0].path().join("file1.txt");
    let snapshot_content = fs::read_to_string(&snapshot_file)?;
    assert_eq!(snapshot_content, "version 1");

    // Verify session has new version
    let session_content = fs::read_to_string(session_dir.join("file1.txt"))?;
    assert_eq!(session_content, "version 2");

    Ok(())
}

#[tokio::test]
async fn test_promote_without_changes() -> Result<()> {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Initialize and spawn
    init::init(repo_path).await?;
    spawn::spawn(repo_path, "agent-1").await?;

    // Try to promote without any changes
    promote::promote(repo_path, "agent-1").await?;

    // Should complete without error, but not create a ref
    let git = GitRepo::open(repo_path)?;
    let vibe_ref = git.get_ref("refs/vibes/agent-1")?;

    // Since there are no dirty files, promote should return early
    // and not create a ref (based on the implementation)
    assert!(vibe_ref.is_none());

    Ok(())
}

#[tokio::test]
async fn test_error_commit_without_promote() -> Result<()> {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Initialize and spawn
    init::init(repo_path).await?;
    spawn::spawn(repo_path, "agent-1").await?;

    // Try to commit without promoting
    let result = commit::commit(repo_path, "agent-1").await;

    // Should fail with appropriate error
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("has not been promoted"));

    Ok(())
}
