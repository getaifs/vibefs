//! `vibe status` command - Show daemon and session status

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use crate::commands::spawn::SpawnInfo;
use crate::daemon_client::DaemonClient;
use crate::daemon_ipc::{self, DaemonResponse};
use crate::db::MetadataStore;
use crate::git::GitRepo;

/// Show status - overview, per-session details, or conflicts
pub async fn status<P: AsRef<Path>>(
    repo_path: P,
    session: Option<&str>,
    show_conflicts: bool,
    json_output: bool,
) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    if !vibe_dir.exists() {
        if json_output {
            println!(r#"{{"error": "VibeFS not initialized"}}"#);
        } else {
            println!("VibeFS not initialized. Run 'vibe init' first.");
        }
        return Ok(());
    }

    if show_conflicts {
        return show_conflicts_status(repo_path, json_output).await;
    }

    if let Some(session_id) = session {
        return show_session_details(repo_path, session_id, json_output).await;
    }

    show_overview(repo_path, json_output).await
}

/// Show overview of daemon and all sessions
async fn show_overview<P: AsRef<Path>>(repo_path: P, json_output: bool) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    // Get current HEAD commit
    let head_commit = GitRepo::open(repo_path)
        .ok()
        .and_then(|git| git.head_commit().ok());

    let mut output = StatusOverview {
        daemon_running: false,
        daemon_pid: None,
        daemon_uptime_secs: None,
        nfs_port: None,
        head_commit: head_commit.clone(),
        active_sessions: Vec::new(),
        offline_sessions: Vec::new(),
    };

    // Check daemon status
    if DaemonClient::is_running(repo_path).await {
        output.daemon_running = true;

        // Read PID
        let pid_path = daemon_ipc::get_pid_path(repo_path);
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            output.daemon_pid = pid_str.trim().parse().ok();
        }

        // Get daemon status
        if let Ok(mut client) = DaemonClient::connect(repo_path).await {
            if let Ok(DaemonResponse::Status { nfs_port, uptime_secs, .. }) = client.status().await {
                output.nfs_port = Some(nfs_port);
                output.daemon_uptime_secs = Some(uptime_secs);
            }

            // Get active sessions
            if let Ok(DaemonResponse::Sessions { sessions }) = client.list_sessions().await {
                for sess in sessions {
                    // Get dirty count from per-session metadata store
                    let dirty_count = {
                        let session_db = vibe_dir.join("sessions").join(&sess.vibe_id).join("metadata.db");
                        let db_path = if session_db.exists() { session_db } else { vibe_dir.join("metadata.db") };
                        if let Ok(store) = MetadataStore::open_readonly(&db_path) {
                            store.get_dirty_paths().map(|p| p.len()).unwrap_or(0)
                        } else {
                            0
                        }
                    };

                    let spawn_info = SpawnInfo::load(repo_path, &sess.vibe_id).ok();
                    let base_commit = spawn_info.as_ref().and_then(|s| s.spawn_commit.clone());
                    let behind_head = match (&base_commit, &head_commit) {
                        (Some(base), Some(head)) => Some(base != head),
                        _ => None,
                    };

                    output.active_sessions.push(SessionSummary {
                        id: sess.vibe_id.clone(),
                        dirty_count,
                        uptime_secs: sess.uptime_secs,
                        mount_point: sess.mount_point,
                        base_commit,
                        behind_head,
                    });
                }
            }
        }
    }

    // Get offline sessions
    let sessions_dir = vibe_dir.join("sessions");
    if sessions_dir.exists() {
        let active_ids: std::collections::HashSet<_> = output
            .active_sessions
            .iter()
            .map(|s| s.id.clone())
            .collect();

        for entry in std::fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.contains("_snapshot_") && !active_ids.contains(&name) {
                    output.offline_sessions.push(name);
                }
            }
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_overview(&output, repo_path);
    }

    Ok(())
}

/// Show details for a specific session
async fn show_session_details<P: AsRef<Path>>(
    repo_path: P,
    session_id: &str,
    json_output: bool,
) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    // Load session info
    let spawn_info = SpawnInfo::load(repo_path, session_id)?;

    // Get current HEAD
    let head_commit = GitRepo::open(repo_path)
        .ok()
        .and_then(|git| git.head_commit().ok());

    // Check if behind HEAD
    let behind_head = match (&spawn_info.spawn_commit, &head_commit) {
        (Some(base), Some(head)) => Some(base != head),
        _ => None,
    };

    // Get dirty files from per-session store (fallback to base)
    let db_path = {
        let session_db = vibe_dir.join("sessions").join(session_id).join("metadata.db");
        if session_db.exists() { session_db } else { vibe_dir.join("metadata.db") }
    };
    let dirty_files = if db_path.exists() {
        match MetadataStore::open_readonly(&db_path) {
            Ok(store) => store.get_dirty_paths()?,
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    // Find snapshots
    let snapshots = find_snapshots(&vibe_dir.join("sessions"), session_id)?;

    // Check daemon for uptime
    let uptime_secs = if DaemonClient::is_running(repo_path).await {
        if let Ok(mut client) = DaemonClient::connect(repo_path).await {
            if let Ok(DaemonResponse::Sessions { sessions }) = client.list_sessions().await {
                sessions
                    .iter()
                    .find(|s| s.vibe_id == session_id)
                    .map(|s| s.uptime_secs)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let output = SessionDetails {
        id: session_id.to_string(),
        mount_point: spawn_info.mount_point.to_string_lossy().to_string(),
        uptime_secs,
        spawn_commit: spawn_info.spawn_commit.clone(),
        head_commit,
        behind_head,
        created_at: spawn_info.created_at.clone(),
        dirty_count: dirty_files.len(),
        dirty_files: dirty_files.clone(),
        snapshots,
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_session_details(&output);
    }

    Ok(())
}

/// Show cross-session file conflicts
async fn show_conflicts_status<P: AsRef<Path>>(repo_path: P, json_output: bool) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    // Get all sessions
    let sessions_dir = vibe_dir.join("sessions");
    let sessions: Vec<String> = if sessions_dir.exists() {
        std::fs::read_dir(&sessions_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| !name.contains("_snapshot_"))
            .collect()
    } else {
        Vec::new()
    };

    // Collect dirty paths per session from per-session metadata stores
    let mut file_sessions: HashMap<String, Vec<String>> = HashMap::new();

    for session in &sessions {
        let session_db = sessions_dir.join(session).join("metadata.db");
        let db_path = if session_db.exists() { session_db } else { vibe_dir.join("metadata.db") };
        if let Ok(store) = MetadataStore::open_readonly(&db_path) {
            if let Ok(dirty_paths) = store.get_dirty_paths() {
                for path in dirty_paths {
                    file_sessions
                        .entry(path)
                        .or_default()
                        .push(session.clone());
                }
            }
        }
    }

    // Filter to only files with multiple sessions
    let conflicts: Vec<ConflictInfo> = file_sessions
        .into_iter()
        .filter(|(_, sessions)| sessions.len() > 1)
        .map(|(path, sessions)| ConflictInfo { path, sessions })
        .collect();

    if json_output {
        println!("{}", serde_json::to_string_pretty(&ConflictsOutput { conflicts: conflicts.clone() })?);
    } else {
        if conflicts.is_empty() {
            println!("No cross-session conflicts detected.");
        } else {
            println!("CROSS-SESSION CONFLICTS:\n");
            for conflict in &conflicts {
                println!("  {}", conflict.path);
                println!("    Modified by: {}\n", conflict.sessions.join(", "));
            }
            println!("RECOMMENDATION: Review conflicts before promoting. Use 'vibe diff <session>' to inspect.");
        }
    }

    Ok(())
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

// Output structs

#[derive(Serialize)]
struct StatusOverview {
    daemon_running: bool,
    daemon_pid: Option<u32>,
    daemon_uptime_secs: Option<u64>,
    nfs_port: Option<u16>,
    head_commit: Option<String>,
    active_sessions: Vec<SessionSummary>,
    offline_sessions: Vec<String>,
}

#[derive(Serialize)]
struct SessionSummary {
    id: String,
    dirty_count: usize,
    uptime_secs: u64,
    mount_point: String,
    base_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    behind_head: Option<bool>,
}

#[derive(Serialize)]
struct SessionDetails {
    id: String,
    mount_point: String,
    uptime_secs: Option<u64>,
    spawn_commit: Option<String>,
    head_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    behind_head: Option<bool>,
    created_at: Option<String>,
    dirty_count: usize,
    dirty_files: Vec<String>,
    snapshots: Vec<String>,
}

#[derive(Serialize, Clone)]
struct ConflictInfo {
    path: String,
    sessions: Vec<String>,
}

#[derive(Serialize)]
struct ConflictsOutput {
    conflicts: Vec<ConflictInfo>,
}

fn print_overview(output: &StatusOverview, repo_path: &Path) {
    println!("VibeFS Status for: {}", repo_path.display());
    println!("================================================================================");

    // Show HEAD commit
    if let Some(ref head) = output.head_commit {
        println!("HEAD: {}", &head[..12.min(head.len())]);
    }

    if output.daemon_running {
        print!("DAEMON: RUNNING");
        if let Some(pid) = output.daemon_pid {
            print!(" (PID: {})", pid);
        }
        println!();

        if let Some(uptime) = output.daemon_uptime_secs {
            println!("  Uptime:       {}", format_uptime(uptime));
        }
        if let Some(port) = output.nfs_port {
            println!("  Global Port:  {}", port);
        }
        println!("  Sessions:     {}", output.active_sessions.len());

        if !output.active_sessions.is_empty() {
            println!("\nACTIVE SESSIONS:");
            for sess in &output.active_sessions {
                let base_short = sess.base_commit.as_ref()
                    .map(|c| &c[..7.min(c.len())])
                    .unwrap_or("unknown");
                let status = match sess.behind_head {
                    Some(true) => " ⚠ BEHIND",
                    Some(false) => "",
                    None => "",
                };
                println!(
                    "  {} [{}] base:{}{} → {}",
                    sess.id,
                    if sess.dirty_count > 0 {
                        format!("{} dirty", sess.dirty_count)
                    } else {
                        "clean".to_string()
                    },
                    base_short,
                    status,
                    sess.mount_point
                );
            }

            // Show warning if any sessions are behind
            let behind_count = output.active_sessions.iter()
                .filter(|s| s.behind_head == Some(true))
                .count();
            if behind_count > 0 {
                println!("\n⚠ {} session(s) behind HEAD. Run 'vibe rebase <session>' to update.", behind_count);
            }
        }
    } else {
        println!("DAEMON: NOT RUNNING");
    }

    if !output.offline_sessions.is_empty() {
        println!("\nOFFLINE SESSIONS (in storage):");
        for session in &output.offline_sessions {
            println!("  - {}", session);
        }
    }

    println!("================================================================================");
}

fn print_session_details(output: &SessionDetails) {
    println!("SESSION: {}", output.id);
    println!("  Mount:     {}", output.mount_point);
    if let Some(uptime) = output.uptime_secs {
        println!("  Uptime:    {}", format_uptime(uptime));
    }

    // Show base commit and HEAD comparison
    if let Some(ref commit) = output.spawn_commit {
        let status = match output.behind_head {
            Some(true) => " ⚠ BEHIND HEAD",
            Some(false) => " ✓ synced",
            None => "",
        };
        println!("  Base:      {}{}", &commit[..12.min(commit.len())], status);
    }
    if let Some(ref head) = output.head_commit {
        if output.behind_head == Some(true) {
            println!("  HEAD:      {}", &head[..12.min(head.len())]);
            println!("\n⚠ Session is behind HEAD. Run 'vibe rebase {}' to update.", output.id);
        }
    }

    println!("  Dirty:     {} files", output.dirty_count);
    if !output.snapshots.is_empty() {
        println!("  Snapshots: {}", output.snapshots.join(", "));
    }

    if !output.dirty_files.is_empty() {
        println!("\nDIRTY FILES:");
        for file in &output.dirty_files {
            println!("  M {}", file);
        }
    }
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(30), "30s");
        assert_eq!(format_uptime(120), "2m");
        assert_eq!(format_uptime(3700), "1h 1m");
        assert_eq!(format_uptime(90000), "1d 1h");
    }
}
