//! `vibe inspect` command - Dump session metadata for debugging

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

use crate::commands::spawn::SpawnInfo;
use crate::db::MetadataStore;
use crate::git::GitRepo;

/// Dump session metadata for debugging
pub async fn inspect<P: AsRef<Path>>(
    repo_path: P,
    session: &str,
    json_output: bool,
) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    // Load session info
    let spawn_info = SpawnInfo::load(repo_path, session)
        .with_context(|| format!("Session '{}' not found. Run 'vibe status' to see active sessions.", session))?;

    // Get dirty files (use read-only mode to avoid lock conflicts with daemon)
    let db_path = vibe_dir.join("metadata.db");
    let dirty_files = if db_path.exists() {
        match MetadataStore::open_readonly(&db_path) {
            Ok(store) => store.get_dirty_paths()?,
            Err(_) => {
                // Fallback: if read-only fails, return empty list
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // Calculate delta size
    let delta_size = calculate_dir_size(&spawn_info.session_dir)?;
    let delta_file_count = count_files(&spawn_info.session_dir)?;

    // Find snapshots
    let snapshots = find_snapshots(&vibe_dir.join("sessions"), session)?;

    // Check for phantom ref
    let phantom_ref = format!("refs/vibes/{}", session);
    let git_repo = GitRepo::open(repo_path)?;
    let phantom_exists = git_repo.get_ref(&phantom_ref)?.is_some();

    // Build output
    let output = InspectOutput {
        session_id: session.to_string(),
        created_at: spawn_info.created_at.clone(),
        mount_point: spawn_info.mount_point.to_string_lossy().to_string(),
        nfs_port: spawn_info.port,
        spawn_commit: spawn_info.spawn_commit.clone(),
        phantom_ref: if phantom_exists { Some(phantom_ref) } else { None },
        delta_path: spawn_info.session_dir.to_string_lossy().to_string(),
        delta_size_bytes: delta_size,
        delta_file_count,
        snapshots,
        dirty_files: dirty_files
            .iter()
            .map(|p| DirtyFile {
                path: p.clone(),
                status: get_file_status(&spawn_info.session_dir.join(p), &spawn_info.spawn_commit, p, &git_repo),
                size_bytes: std::fs::metadata(spawn_info.session_dir.join(p))
                    .map(|m| m.len())
                    .ok(),
            })
            .collect(),
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_human_readable(&output);
    }

    Ok(())
}

#[derive(Serialize)]
struct InspectOutput {
    session_id: String,
    created_at: Option<String>,
    mount_point: String,
    nfs_port: u16,
    spawn_commit: Option<String>,
    phantom_ref: Option<String>,
    delta_path: String,
    delta_size_bytes: u64,
    delta_file_count: usize,
    snapshots: Vec<String>,
    dirty_files: Vec<DirtyFile>,
}

#[derive(Serialize)]
struct DirtyFile {
    path: String,
    status: String,
    size_bytes: Option<u64>,
}

fn print_human_readable(output: &InspectOutput) {
    println!("SESSION: {}\n", output.session_id);

    println!("Metadata:");
    println!("  ID:           {}", output.session_id);
    if let Some(ref created) = output.created_at {
        println!("  Created:      {}", created);
    }
    println!("  Mount Point:  {}", output.mount_point);
    if output.nfs_port > 0 {
        println!("  NFS Port:     {}", output.nfs_port);
    }

    println!("\nGit State:");
    if let Some(ref commit) = output.spawn_commit {
        println!("  Base Commit:  {}", commit);
    } else {
        println!("  Base Commit:  (unknown - old session format)");
    }
    if let Some(ref phantom) = output.phantom_ref {
        println!("  Phantom Ref:  {} (exists)", phantom);
    } else {
        println!("  Phantom Ref:  refs/vibes/{} (not yet promoted)", output.session_id);
    }

    println!("\nStorage:");
    println!("  Delta Path:   {}", output.delta_path);
    println!("  Delta Size:   {} ({} files)",
        format_size(output.delta_size_bytes),
        output.delta_file_count
    );
    if output.snapshots.is_empty() {
        println!("  Snapshots:    (none)");
    } else {
        println!("  Snapshots:    {} ({})",
            output.snapshots.len(),
            output.snapshots.join(", ")
        );
    }

    println!("\nDirty Files ({}):", output.dirty_files.len());
    if output.dirty_files.is_empty() {
        println!("  (no changes)");
    } else {
        for file in &output.dirty_files {
            let size = file.size_bytes
                .map(|s| format!("{}", format_size(s)))
                .unwrap_or_else(|| "-".to_string());
            println!("  {} {:<40} ({})",
                match file.status.as_str() {
                    "new" => "A",
                    "deleted" => "D",
                    _ => "M",
                },
                file.path,
                size
            );
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn calculate_dir_size(path: &Path) -> Result<u64> {
    let mut size = 0u64;

    if !path.exists() {
        return Ok(0);
    }

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;

        if meta.is_file() {
            size += meta.len();
        } else if meta.is_dir() {
            size += calculate_dir_size(&entry.path())?;
        }
    }

    Ok(size)
}

fn count_files(path: &Path) -> Result<usize> {
    let mut count = 0;

    if !path.exists() {
        return Ok(0);
    }

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;

        if meta.is_file() {
            count += 1;
        } else if meta.is_dir() {
            count += count_files(&entry.path())?;
        }
    }

    Ok(count)
}

fn find_snapshots(sessions_dir: &Path, session: &str) -> Result<Vec<String>> {
    let prefix = format!("{}_snapshot_", session);
    let mut snapshots = Vec::new();

    if !sessions_dir.exists() {
        return Ok(snapshots);
    }

    for entry in std::fs::read_dir(sessions_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) {
            let snapshot_name = name.strip_prefix(&prefix).unwrap_or(&name).to_string();
            snapshots.push(snapshot_name);
        }
    }

    snapshots.sort();
    Ok(snapshots)
}

fn get_file_status(session_path: &Path, spawn_commit: &Option<String>, rel_path: &str, git_repo: &GitRepo) -> String {
    let file_exists = session_path.exists();

    // Check if file existed at spawn commit
    let existed_at_spawn = if let Some(ref commit) = spawn_commit {
        let output = std::process::Command::new("git")
            .args(["show", &format!("{}:{}", commit, rel_path)])
            .current_dir(git_repo.repo_path())
            .output();

        output.map(|o| o.status.success()).unwrap_or(false)
    } else {
        false
    };

    match (existed_at_spawn, file_exists) {
        (false, true) => "new".to_string(),
        (true, false) => "deleted".to_string(),
        _ => "modified".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_calculate_dir_size_empty() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let size = calculate_dir_size(temp_dir.path()).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_calculate_dir_size_with_files() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "hello").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "world!").unwrap();

        let size = calculate_dir_size(temp_dir.path()).unwrap();
        assert_eq!(size, 11); // 5 + 6 bytes
    }
}
