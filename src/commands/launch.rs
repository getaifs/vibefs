//! `vibe launch <agent>` command - Spawn session and exec agent

use anyhow::{Context, Result};
use std::os::unix::process::CommandExt;
use std::path::Path;

use crate::commands::spawn::{self, SpawnInfo};
use crate::names;

/// Known agent binaries for shortcuts and "did you mean" suggestions
pub const KNOWN_AGENTS: &[&str] = &[
    "claude", "cursor", "code", "codex", "amp", "aider",
    "nvim", "vim", "emacs", "zed", "hx",
];

/// Check if a string is a known agent name
pub fn is_known_agent(name: &str) -> bool {
    KNOWN_AGENTS.contains(&name)
}

/// Launch an agent in a vibe session
pub async fn launch<P: AsRef<Path>>(
    repo_path: P,
    agent: &str,
    session_name: Option<&str>,
) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");
    let sessions_dir = vibe_dir.join("sessions");

    // Verify agent binary exists in PATH
    let agent_path = which_agent(agent)?;

    // Generate session name if not provided
    let session = match session_name {
        Some(name) => name.to_string(),
        None => names::generate_unique_agent_name(agent, &sessions_dir),
    };

    println!("Launching {} in session '{}'...", agent, session);

    // Spawn the session
    spawn::spawn(repo_path, &session).await?;

    // Load spawn info to get the actual mount point
    let spawn_info = SpawnInfo::load(repo_path, &session)
        .with_context(|| "Failed to load session info after spawn")?;

    let mount_point = spawn_info.mount_point;

    println!("Executing {} in {}", agent, mount_point.display());

    // exec the agent - this replaces the current process
    let err = std::process::Command::new(&agent_path)
        .current_dir(&mount_point)
        .exec();

    // If we get here, exec failed
    Err(anyhow::anyhow!("Failed to exec {}: {}", agent, err))
}

/// Find agent binary in PATH, with helpful error messages
fn which_agent(agent: &str) -> Result<String> {
    // Check if binary exists in PATH
    if let Ok(path) = which::which(agent) {
        return Ok(path.to_string_lossy().to_string());
    }

    // Binary not found - generate helpful error message
    let suggestions = find_similar_agents(agent);

    let mut msg = format!("Binary '{}' not found in PATH.", agent);

    if !suggestions.is_empty() {
        msg.push_str("\nDid you mean: ");
        msg.push_str(&suggestions.join(", "));
        msg.push('?');
    } else {
        msg.push_str("\nKnown agents: ");
        msg.push_str(&KNOWN_AGENTS.join(", "));
    }

    Err(anyhow::anyhow!(msg))
}

/// Find similar agent names using edit distance
fn find_similar_agents(input: &str) -> Vec<String> {
    let mut suggestions: Vec<(String, usize)> = KNOWN_AGENTS
        .iter()
        .filter_map(|&known| {
            let dist = edit_distance(input, known);
            // Only suggest if distance is reasonable (max 3 edits)
            if dist <= 3 {
                Some((known.to_string(), dist))
            } else {
                None
            }
        })
        .collect();

    // Sort by distance
    suggestions.sort_by_key(|(_, dist)| *dist);

    // Return just the names
    suggestions.into_iter().map(|(name, _)| name).collect()
}

/// Simple Levenshtein distance implementation
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Create distance matrix
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    // Initialize base cases
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }

    // Fill the matrix
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1) // deletion
                .min(dp[i][j - 1] + 1)     // insertion
                .min(dp[i - 1][j - 1] + cost); // substitution
        }
    }

    dp[m][n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_distance() {
        assert_eq!(edit_distance("claude", "claude"), 0);
        assert_eq!(edit_distance("cluade", "claude"), 2); // swap
        assert_eq!(edit_distance("claud", "claude"), 1);  // missing e
        assert_eq!(edit_distance("claudee", "claude"), 1); // extra e
        assert_eq!(edit_distance("xyz", "claude"), 6);    // very different
    }

    #[test]
    fn test_find_similar_agents() {
        let suggestions = find_similar_agents("cluade");
        assert!(suggestions.contains(&"claude".to_string()));

        let suggestions = find_similar_agents("codr");
        assert!(suggestions.contains(&"code".to_string()));

        let suggestions = find_similar_agents("xyz123");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_known_agents_list() {
        assert!(KNOWN_AGENTS.contains(&"claude"));
        assert!(KNOWN_AGENTS.contains(&"cursor"));
        assert!(KNOWN_AGENTS.contains(&"aider"));
    }
}
