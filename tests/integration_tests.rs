use anyhow::Result;
use std::fs;
use tempfile::TempDir;
use vibefs::commands::{close, init, promote, snapshot, spawn};
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
    {
        let metadata = MetadataStore::open(repo_path.join(".vibe/metadata.db"))?;
        let root = metadata.get_inode(1)?.unwrap();
        assert!(root.is_dir);
    }

    // Test 2: Spawn a vibe workspace
    spawn::spawn_local(repo_path, "agent-1").await?;

    let session_dir = repo_path.join(".vibe/sessions/agent-1");
    assert!(session_dir.exists());

    // Test 3: Simulate agent modifying files
    fs::write(session_dir.join("new_feature.rs"), "pub fn new_feature() -> bool {\n    true\n}").unwrap();
    fs::write(session_dir.join("README.md"), "# VibeFS Test\n\nUpdated by agent-1").unwrap();

    // Mark files as dirty
    {
        let metadata = MetadataStore::open(repo_path.join(".vibe/metadata.db"))?;
        metadata.mark_dirty("new_feature.rs")?;
        metadata.mark_dirty("README.md")?;

        let dirty_paths = metadata.get_dirty_paths()?;
        assert_eq!(dirty_paths.len(), 2);
    } // Drop metadata before snapshot

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
    promote::promote(repo_path, "agent-1", None, None).await?;

    let git = GitRepo::open(repo_path)?;
    let vibe_ref = git.get_ref("refs/vibes/agent-1")?;
    assert!(vibe_ref.is_some());

    // Test 6: Close the session
    close::close(repo_path, "agent-1", true, false).await?;

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
    spawn::spawn_local(repo_path, "agent-1").await?;
    spawn::spawn_local(repo_path, "agent-2").await?;
    spawn::spawn_local(repo_path, "agent-3").await?;

    // Verify all sessions exist
    assert!(repo_path.join(".vibe/sessions/agent-1").exists());
    assert!(repo_path.join(".vibe/sessions/agent-2").exists());
    assert!(repo_path.join(".vibe/sessions/agent-3").exists());

    // Simulate each agent working on different files
    {
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
    } // Drop metadata before promoting

    // Promote all vibes
    promote::promote(repo_path, "agent-1", None, None).await?;
    promote::promote(repo_path, "agent-2", None, None).await?;
    promote::promote(repo_path, "agent-3", None, None).await?;

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
    spawn::spawn_local(repo_path, "agent-1").await?;

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
    spawn::spawn_local(repo_path, "agent-1").await?;

    // Try to promote without any changes
    promote::promote(repo_path, "agent-1", None, None).await?;

    // Should complete without error, but not create a ref
    let git = GitRepo::open(repo_path)?;
    let vibe_ref = git.get_ref("refs/vibes/agent-1")?;

    // Since there are no dirty files, promote should return early
    // and not create a ref (based on the implementation)
    assert!(vibe_ref.is_none());

    Ok(())
}

#[tokio::test]
async fn test_close_session() -> Result<()> {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Initialize and spawn
    init::init(repo_path).await?;
    spawn::spawn_local(repo_path, "agent-1").await?;

    let session_dir = repo_path.join(".vibe/sessions/agent-1");
    assert!(session_dir.exists());

    // Add some files
    fs::write(session_dir.join("test.txt"), "test content")?;

    // Close the session (force to skip confirmation)
    close::close(repo_path, "agent-1", true, false).await?;

    // Session should be gone
    assert!(!session_dir.exists());

    Ok(())
}

#[tokio::test]
async fn test_close_nonexistent_session() -> Result<()> {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Initialize
    init::init(repo_path).await?;

    // Try to close a session that doesn't exist
    let result = close::close(repo_path, "nonexistent", true, false).await;

    // Should fail
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));

    Ok(())
}

#[tokio::test]
async fn test_list_dirty_files() -> Result<()> {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Initialize and spawn
    init::init(repo_path).await?;
    spawn::spawn_local(repo_path, "agent-1").await?;

    let session_dir = repo_path.join(".vibe/sessions/agent-1");

    // Add some files
    fs::write(session_dir.join("file1.txt"), "content 1")?;
    fs::write(session_dir.join("file2.txt"), "content 2")?;
    fs::create_dir_all(session_dir.join("subdir"))?;
    fs::write(session_dir.join("subdir/file3.txt"), "content 3")?;

    // Get dirty files
    let dirty_files = close::list_dirty(repo_path, "agent-1").await?;

    assert_eq!(dirty_files.len(), 3);
    assert!(dirty_files.iter().any(|f| f == "file1.txt"));
    assert!(dirty_files.iter().any(|f| f == "file2.txt"));
    assert!(dirty_files.iter().any(|f| f.contains("file3.txt")));

    Ok(())
}
