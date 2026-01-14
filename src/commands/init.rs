use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::Path;

use crate::db::{InodeMetadata, MetadataStore};
use crate::git::GitRepo;
use crate::cwd_validation;

const VIBEFS_WORKFLOW_DOCS: &str = r#"
## VibeFS Workflow

This repository uses VibeFS for managing parallel AI agent workflows on Git.

### Core Workflow

1. **Initialize** (first time only):
   ```bash
   vibe init
   ```

2. **Spawn your workspace**:
   ```bash
   vibe spawn <session-name>
   ```

3. **Work in the NFS mount**:
   ```bash
   vibe sh -s <session-name>  # Opens shell in mount
   # Or: vibe launch claude --session <session-name>
   ```
   Changes are automatically tracked.

4. **Promote to Git**:
   ```bash
   vibe promote <session-name>
   ```

5. **Merge to main**:
   ```bash
   git merge refs/vibes/<session-name>
   ```

6. **Close session**:
   ```bash
   vibe close <session-name>
   ```

### Key Commands

- `vibe status` - Show daemon and session status
- `vibe inspect <session>` - Detailed session info
- `vibe diff <session>` - Show changes
- `vibe snapshot <session>` - Create backup
"#;

/// Initialize VibeFS for a Git repository
pub async fn init<P: AsRef<Path>>(repo_path: P) -> Result<()> {
    // Validate that we're running from the correct directory
    let _validated_root = cwd_validation::validate_cwd()
        .context("Cannot initialize VibeFS")?;

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

    // Clear and recreate metadata store to ensure fresh state
    // This prevents stale entries from old Git states
    let metadata_path = vibe_dir.join("metadata.db");
    if metadata_path.exists() {
        std::fs::remove_dir_all(&metadata_path)
            .context("Failed to clear old metadata store")?;
    }
    let metadata = MetadataStore::open(&metadata_path)
        .context("Failed to create metadata store")?;

    println!("Scanning Git repository...");

    // Get HEAD commit
    let head_oid = git.head_commit()
        .context("Failed to get HEAD commit")?;

    // List all files in the tree
    let entries = git.list_tree_files()
        .context("Failed to list tree files")?;

    println!("Found {} file entries", entries.len());

    // Extract all unique directory paths from file paths
    // Git only stores files (blobs), so we need to create directory inodes
    // for all parent directories
    let mut directories: BTreeSet<String> = BTreeSet::new();
    for (path, _) in &entries {
        let mut current = path.as_path();
        while let Some(parent) = current.parent() {
            let parent_str = parent.to_string_lossy().to_string();
            if parent_str.is_empty() {
                break;
            }
            directories.insert(parent_str);
            current = parent;
        }
    }

    println!("Found {} directories", directories.len());

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

    // Create directory inodes first (so parent lookups work during cache building)
    for dir_path in &directories {
        let inode_id = metadata.next_inode_id()?;

        let dir_metadata = InodeMetadata {
            path: dir_path.clone(),
            git_oid: None,  // Directories don't have a git oid
            is_dir: true,
            size: 0,
            volatile: false,
        };

        metadata.put_inode(inode_id, &dir_metadata)?;
    }

    // Populate metadata for all file entries
    for (path, oid) in entries {
        let inode_id = metadata.next_inode_id()?;

        let size = git.read_blob(&oid)
            .map(|data| data.len() as u64)
            .unwrap_or(0);

        let inode_metadata = InodeMetadata {
            path: path.to_string_lossy().to_string(),
            git_oid: Some(oid),
            is_dir: false,
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

    // Bootstrap agent documentation
    bootstrap_agent_docs(repo_path)?;

    Ok(())
}

/// Add VibeFS workflow documentation to the repository
fn bootstrap_agent_docs(repo_path: &Path) -> Result<()> {
    let claude_md = repo_path.join("CLAUDE.md");
    let agents_md = repo_path.join("AGENTS.md");

    if claude_md.exists() {
        // Append to existing CLAUDE.md
        println!("Bootstrapping VibeFS docs in CLAUDE.md...");

        let mut content = std::fs::read_to_string(&claude_md)
            .context("Failed to read CLAUDE.md")?;

        // Check if VibeFS section already exists
        if !content.contains("## VibeFS Workflow") {
            content.push_str("\n\n");
            content.push_str(VIBEFS_WORKFLOW_DOCS);

            std::fs::write(&claude_md, content)
                .context("Failed to write to CLAUDE.md")?;

            println!("✓ Added VibeFS workflow documentation to CLAUDE.md");
        } else {
            println!("  VibeFS workflow docs already present in CLAUDE.md");
        }
    } else {
        // Create new AGENTS.md
        println!("Bootstrapping VibeFS docs in AGENTS.md...");

        let header = "# Agent Workflow Guide\n\nThis guide helps AI agents work effectively with this repository.\n";
        let content = format!("{}{}", header, VIBEFS_WORKFLOW_DOCS);

        std::fs::write(&agents_md, content)
            .context("Failed to create AGENTS.md")?;

        println!("✓ Created AGENTS.md with VibeFS workflow documentation");
    }

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

    #[tokio::test]
    async fn test_init_creates_agents_md() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        init(repo_path).await.unwrap();

        // Should create AGENTS.md since CLAUDE.md doesn't exist
        assert!(repo_path.join("AGENTS.md").exists());

        let content = fs::read_to_string(repo_path.join("AGENTS.md")).unwrap();
        assert!(content.contains("## VibeFS Workflow"));
        assert!(content.contains("vibe spawn"));
        assert!(content.contains("vibe promote"));
    }

    #[tokio::test]
    async fn test_init_appends_to_claude_md() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Create existing CLAUDE.md
        fs::write(repo_path.join("CLAUDE.md"), "# Existing Content\n\nSome docs here.").unwrap();

        init(repo_path).await.unwrap();

        // Should append to CLAUDE.md
        assert!(repo_path.join("CLAUDE.md").exists());
        assert!(!repo_path.join("AGENTS.md").exists());

        let content = fs::read_to_string(repo_path.join("CLAUDE.md")).unwrap();
        assert!(content.contains("# Existing Content"));
        assert!(content.contains("## VibeFS Workflow"));
        assert!(content.contains("vibe spawn"));
    }

    #[tokio::test]
    async fn test_init_idempotent() {
        let temp_dir = setup_test_repo();
        let repo_path = temp_dir.path();

        // Run init twice
        init(repo_path).await.unwrap();
        init(repo_path).await.unwrap();

        // Should not duplicate the docs
        let content = fs::read_to_string(repo_path.join("AGENTS.md")).unwrap();
        let count = content.matches("## VibeFS Workflow").count();
        assert_eq!(count, 1, "VibeFS Workflow section should appear exactly once");
    }
}
