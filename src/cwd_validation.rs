use anyhow::{anyhow, Context, Result};
use std::env;
use std::path::{Path, PathBuf};

/// Validates that the current working directory is appropriate for running vibe commands.
///
/// Returns the repository root path on success.
/// Returns an error with helpful hints if:
/// - Not in a Git repository
/// - In a Git subdirectory (not at root)
/// - Inside a vibe session directory
pub fn validate_cwd() -> Result<PathBuf> {
    let current_dir = env::current_dir()
        .context("Failed to get current working directory")?;

    // If we're inside a vibe mount, use the original repo path and allow any subdir
    if let Some(origin) = crate::platform::detect_vibe_mount_origin(&current_dir) {
        eprintln!(
            "Info: Detected VibeFS mount. Using repository at: {}",
            origin.display()
        );
        return Ok(origin);
    }

    // If this looks like a mount path but isn't registered, give a friendly warning
    let mounts_dir = crate::platform::get_vibe_mounts_dir();
    if current_dir.starts_with(&mounts_dir) {
        return Err(anyhow!(
            "Error: This path looks like a VibeFS mount, but it isn't registered\n\n\
            Current directory: {}\n\
            Mounts directory:  {}\n\n\
            Hint: Re-run 'vibe new <session>' from the repo root, or run\n\
            'vibe status' from the repo root to repair the mount registry.",
            current_dir.display(),
            mounts_dir.display()
        ));
    }

    // Find repository root without invoking git
    let repo_root = find_repo_root(&current_dir)?;

    // Check if we're inside a session directory (most problematic)
    if is_in_session_directory(&current_dir) {
        return Err(anyhow!(
            "Error: Cannot run vibe commands from inside a session directory\n\n\
            Current directory: {}\n\
            Repository root:  {}\n\n\
            This creates nested .vibe directories and breaks the workflow!\n\n\
            Hint: Always run vibe commands from the repository root:\n  \
            cd {}\n  \
            vibe <command>\n\n\
            Reminder: Session directories are workspaces for editing files.\n\
            Use absolute paths when working in sessions, but run vibe\n\
            commands from the repository root.",
            current_dir.display(),
            repo_root.display(),
            repo_root.display()
        ));
    }

    // Check if we're at the repository root
    let current_dir_canonical = current_dir.canonicalize()
        .context("Failed to canonicalize current directory")?;
    let repo_root_canonical = repo_root.canonicalize()
        .context("Failed to canonicalize repository root")?;

    if current_dir_canonical != repo_root_canonical {
        return Err(anyhow!(
            "Error: Must run vibe commands from repository root\n\n\
            Current directory: {}\n\
            Repository root:  {}\n\n\
            Hint: Navigate to the repository root:\n  \
            cd {}\n  \
            vibe <command>",
            current_dir.display(),
            repo_root.display(),
            repo_root.display()
        ));
    }

    Ok(repo_root)
}

/// Finds the git repository root by walking up to a directory containing .git
fn find_repo_root(from_dir: &Path) -> Result<PathBuf> {
    let mut current = from_dir;

    loop {
        let git_path = current.join(".git");
        if git_path.exists() {
            return Ok(current.to_path_buf());
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    Err(anyhow!(
        "Error: Current directory is not a Git repository\n\n\
        Current directory: {}\n\n\
        VibeFS requires a Git repository to operate.\n\n\
        Hint: Navigate to your Git repository root before running vibe commands:\n  \
        cd /path/to/your/repo\n  \
        vibe init",
        from_dir.display()
    ))
}

/// Checks if the current directory is inside a .vibe/sessions/ directory
fn is_in_session_directory(path: &Path) -> bool {
    path.to_str()
        .map(|s| s.contains("/.vibe/sessions/"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn test_is_in_session_directory() {
        let session_path = PathBuf::from("/home/user/repo/.vibe/sessions/agent-1");
        assert!(is_in_session_directory(&session_path));

        let normal_path = PathBuf::from("/home/user/repo/src");
        assert!(!is_in_session_directory(&normal_path));

        let root_path = PathBuf::from("/home/user/repo");
        assert!(!is_in_session_directory(&root_path));
    }

    #[test]
    fn test_find_repo_root_not_in_repo() {
        let temp = TempDir::new().unwrap();
        let result = find_repo_root(temp.path());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("not a Git repository"));
    }

    #[test]
    fn test_find_repo_root_in_repo() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path();

        // Initialize a git repo
        Command::new("git")
            .args(&["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let result = find_repo_root(repo_path);
        assert!(result.is_ok());
        let root = result.unwrap();
        assert_eq!(root.canonicalize().unwrap(), repo_path.canonicalize().unwrap());
    }

    #[test]
    fn test_find_repo_root_in_subdirectory() {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path();

        // Initialize a git repo
        Command::new("git")
            .args(&["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create a subdirectory
        let subdir = repo_path.join("src").join("commands");
        fs::create_dir_all(&subdir).unwrap();

        let result = find_repo_root(&subdir);
        assert!(result.is_ok());
        let root = result.unwrap();
        assert_eq!(root.canonicalize().unwrap(), repo_path.canonicalize().unwrap());
    }
}
