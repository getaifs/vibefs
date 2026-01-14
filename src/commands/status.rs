//! `vibe status` command - Show daemon and session status

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use crate::commands::spawn::SpawnInfo;
use crate::daemon_client::DaemonClient;
use crate::daemon_ipc::{self, DaemonResponse};
use crate::db::MetadataStore;

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

    let mut output = StatusOverview {
        daemon_running: false,
        daemon_pid: None,
        daemon_uptime_secs: None,
        nfs_port: None,
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
                // Get dirty counts for each session
                let db_path = vibe_dir.join("metadata.db");
                let dirty_counts = if db_path.exists() {
                    if let Ok(store) = MetadataStore::open_readonly(&db_path) {
                        get_dirty_counts_by_session(&store, repo_path)
                    } else {
                        HashMap::new()
                    }
                } else {
                    HashMap::new()
                };

                for sess in sessions {
                    output.active_sessions.push(SessionSummary {
                        id: sess.vibe_id.clone(),
                        dirty_count: dirty_counts.get(&sess.vibe_id).copied().unwrap_or(0),
                        uptime_secs: sess.uptime_secs,
                        mount_point: sess.mount_point,
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

    // Get dirty files (read-only to avoid lock conflicts with daemon)
    let db_path = vibe_dir.join("metadata.db");
    let dirty_files = if db_path.exists() {
        match MetadataStore::open_readonly(&db_path) {
            Ok(store) => store.get_dirty_paths()?,
            Err(_) => Vec::new(), // Fallback if read-only fails
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

    let db_path = vibe_dir.join("metadata.db");
    if !db_path.exists() {
        if json_output {
            println!(r#"{{"conflicts": []}}"#);
        } else {
            println!("No conflicts detected.");
        }
        return Ok(());
    }

    let store = MetadataStore::open_readonly(&db_path)
        .map_err(|_| anyhow::anyhow!("Cannot open metadata store"))?;
    let dirty_paths = store.get_dirty_paths()?;

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

    // Check which sessions have each dirty file
    let mut file_sessions: HashMap<String, Vec<String>> = HashMap::new();

    for path in dirty_paths {
        for session in &sessions {
            let session_file = sessions_dir.join(session).join(&path);
            if session_file.exists() {
                file_sessions
                    .entry(path.clone())
                    .or_default()
                    .push(session.clone());
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

fn get_dirty_counts_by_session(store: &MetadataStore, _repo_path: &Path) -> HashMap<String, usize> {
    // For now, return total dirty count for all sessions
    // TODO: Track dirty files per session in metadata store
    let mut counts = HashMap::new();
    if let Ok(dirty) = store.get_dirty_paths() {
        // Assume single session for now
        counts.insert("default".to_string(), dirty.len());
    }
    counts
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
    active_sessions: Vec<SessionSummary>,
    offline_sessions: Vec<String>,
}

#[derive(Serialize)]
struct SessionSummary {
    id: String,
    dirty_count: usize,
    uptime_secs: u64,
    mount_point: String,
}

#[derive(Serialize)]
struct SessionDetails {
    id: String,
    mount_point: String,
    uptime_secs: Option<u64>,
    spawn_commit: Option<String>,
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
            println!("  {:<20} {:<8} {:<12} {:<40}", "SESSION", "DIRTY", "UPTIME", "MOUNT");
            println!("  {:-<20} {:-<8} {:-<12} {:-<40}", "", "", "", "");
            for sess in &output.active_sessions {
                println!(
                    "  {:<20} {:<8} {:<12} {:<40}",
                    sess.id,
                    sess.dirty_count,
                    format_uptime(sess.uptime_secs),
                    sess.mount_point
                );
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
    if let Some(ref commit) = output.spawn_commit {
        println!("  Base:      {} ", &commit[..12.min(commit.len())]);
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
