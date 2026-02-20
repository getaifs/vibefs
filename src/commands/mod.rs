pub mod init;
pub mod spawn;
pub mod snapshot;
pub mod promote;
pub mod purge;
pub mod close;
pub mod diff;
pub mod restore;
pub mod inspect;
pub mod status;
pub mod launch;
pub mod rebase;

use anyhow::{Context, Result};
use std::path::Path;

/// Detection source for session auto-detection
#[derive(Debug, Clone, PartialEq)]
pub enum DetectionSource {
    /// Session detected from mount path
    Mount,
    /// Session detected as the only active session
    OnlySession,
}

/// Result of session detection
#[derive(Debug, Clone)]
pub struct DetectedSession {
    pub session_id: String,
    pub source: DetectionSource,
}

/// Detect the current session from the working directory.
///
/// Returns `Some(DetectedSession)` if:
/// - cwd is inside a vibe mount point, OR
/// - there's exactly one active session
///
/// Returns `None` if detection fails (multiple sessions, not in mount, etc.)
pub fn detect_current_session(repo_path: &Path) -> Result<Option<DetectedSession>> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    // Get the vibe mounts directory
    let mounts_dir = crate::platform::get_vibe_mounts_dir();

    // Check if cwd is inside a mount point
    if cwd.starts_with(&mounts_dir) {
        // Extract session from path: mounts_dir/<repo-name>-<session>/...
        if let Some(relative) = cwd.strip_prefix(&mounts_dir).ok() {
            if let Some(first_component) = relative.components().next() {
                let mount_name = first_component.as_os_str().to_string_lossy();
                // Mount name format: <repo-name>-<session>
                // Extract session ID (everything after the last hyphen that matches a session)
                let sessions_dir = repo_path.join(".vibe/sessions");
                if sessions_dir.exists() {
                    // List all session directories and find which one matches
                    for entry in std::fs::read_dir(&sessions_dir)? {
                        let entry = entry?;
                        if entry.file_type()?.is_dir() {
                            let session_id = entry.file_name().to_string_lossy().to_string();
                            if mount_name.ends_with(&format!("-{}", session_id)) {
                                return Ok(Some(DetectedSession {
                                    session_id,
                                    source: DetectionSource::Mount,
                                }));
                            }
                        }
                    }
                }
            }
        }
    }

    // Not in a mount - check if there's exactly one active session
    let sessions_dir = repo_path.join(".vibe/sessions");
    if sessions_dir.exists() {
        let mut session_dirs: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip snapshot directories (they contain underscore timestamps)
                if !name.contains('_') || !name.chars().any(|c| c.is_ascii_digit()) {
                    session_dirs.push(name);
                }
            }
        }

        if session_dirs.len() == 1 {
            return Ok(Some(DetectedSession {
                session_id: session_dirs.remove(0),
                source: DetectionSource::OnlySession,
            }));
        }
    }

    Ok(None)
}

/// Get session or return error with helpful message listing available sessions.
/// Prints info message when session is auto-detected.
pub fn require_session(repo_path: &Path, session: Option<String>) -> Result<String> {
    if let Some(s) = session {
        return Ok(s);
    }

    // Try auto-detection
    if let Some(detected) = detect_current_session(repo_path)? {
        match detected.source {
            DetectionSource::Mount => {
                eprintln!("Using session '{}' (detected from mount)", detected.session_id);
            }
            DetectionSource::OnlySession => {
                eprintln!("Using session '{}' (only active session)", detected.session_id);
            }
        }
        return Ok(detected.session_id);
    }

    // Build error message with available sessions
    let sessions_dir = repo_path.join(".vibe/sessions");
    let mut sessions: Vec<String> = Vec::new();

    if sessions_dir.exists() {
        for entry in std::fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip snapshot directories
                if !name.contains('_') || !name.chars().any(|c| c.is_ascii_digit()) {
                    sessions.push(name);
                }
            }
        }
    }

    if sessions.is_empty() {
        anyhow::bail!("No sessions exist.\n\nRun 'vibe new' to create one.");
    }

    let session_list = sessions.iter()
        .map(|s| format!("  {}", s))
        .collect::<Vec<_>>()
        .join("\n");

    anyhow::bail!(
        "No session specified.\n\n\
         Active sessions:\n{}\n\n\
         Hint: Specify a session name, or run from inside a vibe mount.",
        session_list
    );
}
