// Simplified Git operations using the git command
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Simplified Git repository interface
pub struct GitRepo {
    repo_path: PathBuf,
}

impl GitRepo {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo_path = path.as_ref().to_path_buf();

        // Verify it's a git repo
        let output = Command::new("git")
            .args(&["rev-parse", "--git-dir"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to run git command")?;

        if !output.status.success() {
            anyhow::bail!("Not a git repository");
        }

        Ok(Self { repo_path })
    }

    /// Get the repository path
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    pub fn head_commit(&self) -> Result<String> {
        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to get HEAD commit")?;

        if !output.status.success() {
            anyhow::bail!("Failed to get HEAD");
        }

        let oid = String::from_utf8(output.stdout)?
            .trim()
            .to_string();
        Ok(oid)
    }

    pub fn read_blob(&self, oid: &str) -> Result<Vec<u8>> {
        let output = Command::new("git")
            .args(&["cat-file", "blob", oid])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to read blob")?;

        if !output.status.success() {
            anyhow::bail!("Failed to read blob {}", oid);
        }

        Ok(output.stdout)
    }

    pub fn write_blob(&self, data: &[u8]) -> Result<String> {
        let mut child = Command::new("git")
            .args(&["hash-object", "-w", "--stdin"])
            .current_dir(&self.repo_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn git hash-object")?;

        {
            use std::io::Write;
            let stdin = child.stdin.as_mut().unwrap();
            stdin.write_all(data)?;
        }

        let output = child.wait_with_output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to write blob");
        }

        let oid = String::from_utf8(output.stdout)?
            .trim()
            .to_string();
        Ok(oid)
    }

    pub fn list_tree_files(&self) -> Result<Vec<(PathBuf, String)>> {
        let output = Command::new("git")
            .args(&["ls-tree", "-r", "HEAD"])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to list tree files")?;

        if !output.status.success() {
            anyhow::bail!("Failed to list tree");
        }

        let stdout = String::from_utf8(output.stdout)?;
        let mut files = Vec::new();

        for line in stdout.lines() {
            // Format: <mode> <type> <hash>\t<path>
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 2 {
                continue;
            }

            let metadata: Vec<&str> = parts[0].split_whitespace().collect();
            if metadata.len() != 3 {
                continue;
            }

            let oid = metadata[2].to_string();
            let path = PathBuf::from(parts[1]);

            files.push((path, oid));
        }

        Ok(files)
    }

    /// Read file content at a specific commit (like `git show <commit>:<path>`)
    pub fn read_file_at_commit(&self, commit: &str, path: &str) -> Result<Option<Vec<u8>>> {
        let spec = format!("{}:{}", commit, path);
        let output = Command::new("git")
            .args(&["show", &spec])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to run git show")?;

        if !output.status.success() {
            // File doesn't exist at this commit
            return Ok(None);
        }

        Ok(Some(output.stdout))
    }

    pub fn update_ref(&self, refname: &str, oid: &str) -> Result<()> {
        let output = Command::new("git")
            .args(&["update-ref", refname, oid])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to update ref")?;

        if !output.status.success() {
            anyhow::bail!("Failed to update ref");
        }

        Ok(())
    }

    pub fn get_ref(&self, refname: &str) -> Result<Option<String>> {
        let output = Command::new("git")
            .args(&["rev-parse", "--verify", refname])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to get ref")?;

        if !output.status.success() {
            return Ok(None);
        }

        let oid = String::from_utf8(output.stdout)?
            .trim()
            .to_string();
        Ok(Some(oid))
    }

    pub fn create_commit(&self, tree_oid: &str, parent_oid: &str, message: &str) -> Result<String> {
        let output = Command::new("git")
            .args(&["commit-tree", tree_oid, "-p", parent_oid, "-m", message])
            .current_dir(&self.repo_path)
            .output()
            .context("Failed to create commit")?;

        if !output.status.success() {
            anyhow::bail!("Failed to create commit");
        }

        let oid = String::from_utf8(output.stdout)?
            .trim()
            .to_string();
        Ok(oid)
    }
}
