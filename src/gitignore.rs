//! Gitignore pattern matching for VibeFS
//!
//! This module handles filtering files based on .gitignore rules.
//! It's used during promotion to exclude build artifacts, dependencies,
//! and other files that shouldn't be committed to Git.

use anyhow::Result;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::Path;

/// Filter that determines which files should be excluded from promotion
pub struct PromoteFilter {
    gitignore: Option<Gitignore>,
    repo_path: std::path::PathBuf,
}

impl PromoteFilter {
    /// Create a new filter by loading .gitignore from the repository
    ///
    /// Loads .gitignore from:
    /// 1. Session directory (if modified)
    /// 2. Repository root (fallback)
    pub fn new<P: AsRef<Path>>(repo_path: P, session_dir: Option<&Path>) -> Result<Self> {
        let repo_path = repo_path.as_ref();

        // Try to load .gitignore - first from session (if modified), then from repo
        let gitignore_content = if let Some(session) = session_dir {
            let session_gitignore = session.join(".gitignore");
            if session_gitignore.exists() {
                std::fs::read_to_string(&session_gitignore).ok()
            } else {
                None
            }
        } else {
            None
        };

        // Fall back to repo .gitignore
        let gitignore_content = gitignore_content.or_else(|| {
            let repo_gitignore = repo_path.join(".gitignore");
            if repo_gitignore.exists() {
                std::fs::read_to_string(&repo_gitignore).ok()
            } else {
                None
            }
        });

        // Build the gitignore matcher
        let gitignore = if let Some(content) = gitignore_content {
            let mut builder = GitignoreBuilder::new(repo_path);

            // Add each line from .gitignore
            for line in content.lines() {
                // Skip empty lines and comments
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    // Add the pattern - the ignore crate handles the globbing
                    if let Err(e) = builder.add_line(None, line) {
                        eprintln!("Warning: invalid gitignore pattern '{}': {}", line, e);
                    }
                }
            }

            match builder.build() {
                Ok(gi) => Some(gi),
                Err(e) => {
                    eprintln!("Warning: failed to build gitignore matcher: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            gitignore,
            repo_path: repo_path.to_path_buf(),
        })
    }

    /// Check if a path should be ignored (excluded from promotion)
    pub fn is_ignored(&self, path: &str) -> bool {
        // Always ignore these special files
        if is_always_ignored(path) {
            return true;
        }

        // Check against .gitignore patterns
        if let Some(ref gitignore) = self.gitignore {
            // Build full path for matching
            let full_path = self.repo_path.join(path);

            // Determine if this is a directory (heuristic based on path patterns)
            let is_dir = path.ends_with('/') || is_likely_directory(path);

            // Try matching as-is first
            match gitignore.matched(&full_path, is_dir) {
                ignore::Match::Ignore(_) => return true,
                ignore::Match::Whitelist(_) => return false,
                ignore::Match::None => {}
            }

            // For paths inside directories like node_modules/foo.js,
            // also check if any parent directory is ignored
            let path_parts: Vec<&str> = path.split('/').collect();
            for i in 1..path_parts.len() {
                let parent = path_parts[..i].join("/");
                let parent_path = self.repo_path.join(&parent);
                // Parent directories should be checked as directories
                match gitignore.matched(&parent_path, true) {
                    ignore::Match::Ignore(_) => return true,
                    ignore::Match::Whitelist(_) => return false,
                    ignore::Match::None => {}
                }
            }

            false
        } else {
            // No gitignore - fall back to common patterns
            is_commonly_ignored(path)
        }
    }

    /// Filter a list of paths, returning only those that should be promoted
    pub fn filter_promotable<'a>(&self, paths: &'a [String]) -> Vec<&'a String> {
        paths.iter().filter(|p| !self.is_ignored(p)).collect()
    }

    /// Filter a list of paths, returning (promotable, ignored)
    pub fn partition_paths<'a>(&self, paths: &'a [String]) -> (Vec<&'a String>, Vec<&'a String>) {
        let (ignored, promotable): (Vec<_>, Vec<_>) =
            paths.iter().partition(|p| self.is_ignored(p));
        (promotable, ignored)
    }
}

/// Check if a path is likely a directory based on common patterns
fn is_likely_directory(path: &str) -> bool {
    // Common directory names that are typically gitignored
    let dir_patterns = [
        "node_modules",
        "__pycache__",
        ".pytest_cache",
        ".mypy_cache",
        "target",
        "dist",
        "build",
        ".git",
        ".venv",
        "venv",
        ".env", // as a directory
        "coverage",
        ".nyc_output",
    ];

    for pattern in &dir_patterns {
        if path == *pattern || path.starts_with(&format!("{}/", pattern)) {
            return true;
        }
        // Check for pattern anywhere in path
        if path.contains(&format!("/{}/", pattern)) {
            return true;
        }
    }

    false
}

/// Files that are always ignored regardless of .gitignore
fn is_always_ignored(path: &str) -> bool {
    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // macOS metadata files
    if filename.starts_with("._") {
        return true;
    }
    if filename == ".DS_Store" {
        return true;
    }

    // Git internal files
    if path.starts_with(".git/") || path == ".git" {
        return true;
    }

    // VibeFS internal files (shouldn't be promoted)
    if path.starts_with(".vibe/") || path == ".vibe" {
        return true;
    }

    false
}

/// Common patterns that are typically gitignored
/// Used as fallback if no .gitignore exists
pub fn is_commonly_ignored(path: &str) -> bool {
    // Check filename components
    let parts: Vec<&str> = path.split('/').collect();

    for part in &parts {
        // Node.js
        if *part == "node_modules" {
            return true;
        }
        // Python
        if *part == "__pycache__" || *part == ".pytest_cache" || *part == ".mypy_cache" {
            return true;
        }
        if *part == ".venv" || *part == "venv" {
            return true;
        }
        // Rust - be more careful with target, only if at root
        if *part == "target" && (path.starts_with("target/") || path == "target") {
            return true;
        }
        // General build output at root
        if (*part == "dist" || *part == "build" || *part == "out")
            && (path.starts_with(&format!("{}/", part)) || path == *part)
        {
            return true;
        }
        // Coverage and test output
        if *part == "coverage" || *part == ".coverage" || *part == ".nyc_output" {
            return true;
        }
    }

    // Check file extensions
    if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
        match ext {
            // Compiled files
            "pyc" | "pyo" | "o" | "obj" | "class" => return true,
            // Log files
            "log" => return true,
            _ => {}
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_always_ignored() {
        assert!(is_always_ignored(".DS_Store"));
        assert!(is_always_ignored("._metadata"));
        assert!(is_always_ignored(".git/config"));
        assert!(is_always_ignored(".vibe/sessions/test"));
        assert!(!is_always_ignored("src/main.rs"));
        assert!(!is_always_ignored("README.md"));
    }

    #[test]
    fn test_commonly_ignored() {
        assert!(is_commonly_ignored("node_modules/foo/bar.js"));
        assert!(is_commonly_ignored("__pycache__/module.pyc"));
        assert!(is_commonly_ignored("target/debug/binary"));
        assert!(!is_commonly_ignored("src/main.rs"));
        assert!(!is_commonly_ignored("package.json"));
    }

    #[test]
    fn test_filter_with_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        let gitignore_path = temp_dir.path().join(".gitignore");

        // Write gitignore with patterns
        std::fs::write(&gitignore_path, "node_modules/\n*.log\nbuild/\n").unwrap();

        let filter = PromoteFilter::new(temp_dir.path(), None).unwrap();

        // These should be ignored by gitignore patterns
        assert!(
            filter.is_ignored("node_modules/foo.js"),
            "node_modules/foo.js should be ignored"
        );
        assert!(filter.is_ignored("app.log"), "app.log should be ignored");
        assert!(
            filter.is_ignored("build/output.js"),
            "build/output.js should be ignored"
        );

        // These should NOT be ignored
        assert!(
            !filter.is_ignored("src/main.rs"),
            "src/main.rs should not be ignored"
        );
        assert!(
            !filter.is_ignored("package.json"),
            "package.json should not be ignored"
        );
    }

    #[test]
    fn test_partition_paths() {
        let temp_dir = TempDir::new().unwrap();
        let gitignore_path = temp_dir.path().join(".gitignore");

        std::fs::write(&gitignore_path, "node_modules/\n*.log\n").unwrap();

        let filter = PromoteFilter::new(temp_dir.path(), None).unwrap();

        let paths = vec![
            "src/main.rs".to_string(),
            "node_modules/foo.js".to_string(),
            "README.md".to_string(),
            "debug.log".to_string(),
        ];

        let (promotable, ignored) = filter.partition_paths(&paths);

        assert_eq!(
            promotable.len(),
            2,
            "Expected 2 promotable files, got {:?}",
            promotable
        );
        assert!(promotable.contains(&&"src/main.rs".to_string()));
        assert!(promotable.contains(&&"README.md".to_string()));

        assert_eq!(
            ignored.len(),
            2,
            "Expected 2 ignored files, got {:?}",
            ignored
        );
        assert!(ignored.contains(&&"node_modules/foo.js".to_string()));
        assert!(ignored.contains(&&"debug.log".to_string()));
    }

    #[test]
    fn test_no_gitignore_uses_common_patterns() {
        let temp_dir = TempDir::new().unwrap();
        // No .gitignore file

        let filter = PromoteFilter::new(temp_dir.path(), None).unwrap();

        // Always-ignored files should still be ignored
        assert!(filter.is_ignored(".DS_Store"));
        assert!(filter.is_ignored(".git/config"));

        // Common patterns should be ignored as fallback
        assert!(
            filter.is_ignored("node_modules/foo.js"),
            "node_modules should be ignored by common patterns"
        );
        assert!(
            filter.is_ignored("__pycache__/foo.pyc"),
            "__pycache__ should be ignored by common patterns"
        );

        // Normal files should not be ignored
        assert!(!filter.is_ignored("src/main.rs"));
    }

    #[test]
    fn test_session_gitignore_override() {
        let temp_dir = TempDir::new().unwrap();
        let session_dir = temp_dir.path().join("session");
        std::fs::create_dir(&session_dir).unwrap();

        // Repo .gitignore ignores *.log
        std::fs::write(temp_dir.path().join(".gitignore"), "*.log\n").unwrap();

        // Session .gitignore also ignores *.tmp
        std::fs::write(session_dir.join(".gitignore"), "*.log\n*.tmp\n").unwrap();

        let filter = PromoteFilter::new(temp_dir.path(), Some(&session_dir)).unwrap();

        // Should use session's .gitignore
        assert!(filter.is_ignored("app.log"));
        assert!(filter.is_ignored("cache.tmp"));
    }

    #[test]
    fn test_is_likely_directory() {
        assert!(is_likely_directory("node_modules"));
        assert!(is_likely_directory("node_modules/foo"));
        assert!(is_likely_directory("__pycache__"));
        assert!(is_likely_directory("target"));
        assert!(is_likely_directory("target/debug"));
        assert!(!is_likely_directory("src"));
        assert!(!is_likely_directory("main.rs"));
    }
}
