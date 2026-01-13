//! `vibe diff` command - Show unified diff of session changes

use anyhow::{Context, Result};
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::commands::spawn::SpawnInfo;
use crate::db::MetadataStore;
use crate::git::GitRepo;

/// Show unified diff of session changes against base commit
pub async fn diff<P: AsRef<Path>>(
    repo_path: P,
    session: &str,
    stat_only: bool,
    color: ColorOption,
    no_pager: bool,
) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    // Load session info
    let spawn_info = SpawnInfo::load(repo_path, session)
        .with_context(|| format!("Session '{}' not found. Run 'vibe status' to see active sessions.", session))?;

    // Get spawn commit
    let spawn_commit = spawn_info.spawn_commit.ok_or_else(|| {
        anyhow::anyhow!(
            "Session '{}' has no spawn commit recorded. Cannot compute diff.\n\
             This may be a session created with an older version of VibeFS.",
            session
        )
    })?;

    // Open metadata store to get dirty files
    let db_path = vibe_dir.join("metadata.db");
    let store = MetadataStore::open(&db_path)?;
    let dirty_paths = store.get_dirty_paths()?;

    if dirty_paths.is_empty() {
        println!("No changes in session '{}'", session);
        return Ok(());
    }

    // Build the diff output
    let git_repo = GitRepo::open(repo_path)?;
    let session_dir = spawn_info.session_dir;

    let mut diff_output = String::new();

    for path in &dirty_paths {
        let path_str = path.as_str();

        // Get base content from spawn commit
        let base_content = get_file_at_commit(&git_repo, &spawn_commit, &path_str);

        // Get current content from session
        let session_file = session_dir.join(path_str);
        let current_content = if session_file.exists() {
            std::fs::read(&session_file).ok()
        } else {
            None
        };

        // Determine file status
        let (status, a_content, b_content) = match (&base_content, &current_content) {
            (None, Some(content)) => ("new file", Vec::new(), content.clone()),
            (Some(content), None) => ("deleted", content.clone(), Vec::new()),
            (Some(base), Some(curr)) => ("modified", base.clone(), curr.clone()),
            (None, None) => continue, // File doesn't exist in either - skip
        };

        // Check if binary
        if is_binary(&a_content) || is_binary(&b_content) {
            diff_output.push_str(&format!(
                "Binary file {} ({}).\n",
                path_str, status
            ));
            continue;
        }

        // Generate unified diff
        let a_text = String::from_utf8_lossy(&a_content);
        let b_text = String::from_utf8_lossy(&b_content);

        if stat_only {
            let additions = b_text.lines().count();
            let deletions = a_text.lines().count();
            diff_output.push_str(&format!(
                " {} | {} {}{}\n",
                path_str,
                additions + deletions,
                "+".repeat(additions.min(40)),
                "-".repeat(deletions.min(40))
            ));
        } else {
            diff_output.push_str(&format!("diff --vibe a/{} b/{}\n", path_str, path_str));

            if status == "new file" {
                diff_output.push_str("new file mode 100644\n");
            } else if status == "deleted" {
                diff_output.push_str("deleted file mode 100644\n");
            }

            diff_output.push_str(&format!("--- a/{}\n", path_str));
            diff_output.push_str(&format!("+++ b/{}\n", path_str));

            // Generate unified diff hunks
            let diff_lines = generate_unified_diff(&a_text, &b_text);
            diff_output.push_str(&diff_lines);
            diff_output.push('\n');
        }
    }

    if stat_only {
        diff_output.push_str(&format!(
            "\n {} files changed\n",
            dirty_paths.len()
        ));
    }

    // Apply coloring if needed
    let should_color = match color {
        ColorOption::Always => true,
        ColorOption::Never => false,
        ColorOption::Auto => std::io::stdout().is_terminal(),
    };

    let colored_output = if should_color {
        colorize_diff(&diff_output)
    } else {
        diff_output
    };

    // Output via pager or directly
    if !no_pager && std::io::stdout().is_terminal() && colored_output.lines().count() > 25 {
        output_with_pager(&colored_output)?;
    } else {
        print!("{}", colored_output);
    }

    Ok(())
}

/// Get file content at a specific commit
fn get_file_at_commit(git_repo: &GitRepo, commit: &str, path: &str) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args(["show", &format!("{}:{}", commit, path)])
        .current_dir(&git_repo.repo_path())
        .output()
        .ok()?;

    if output.status.success() {
        Some(output.stdout)
    } else {
        None
    }
}

/// Check if content is binary (contains null bytes)
fn is_binary(content: &[u8]) -> bool {
    content.iter().take(8000).any(|&b| b == 0)
}

/// Generate unified diff between two strings
fn generate_unified_diff(a: &str, b: &str) -> String {
    use std::fmt::Write;

    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();

    // Simple line-by-line diff
    let mut output = String::new();

    // Find changed regions
    let max_len = a_lines.len().max(b_lines.len());
    let mut i = 0;

    while i < max_len {
        let a_line = a_lines.get(i).copied();
        let b_line = b_lines.get(i).copied();

        if a_line != b_line {
            // Found a difference - emit hunk
            let hunk_start = i.saturating_sub(3);
            let mut hunk_end = i;

            // Find end of changed region
            while hunk_end < max_len {
                let a_l = a_lines.get(hunk_end).copied();
                let b_l = b_lines.get(hunk_end).copied();
                if a_l == b_l {
                    // Found matching line, check if we have 3+ context lines
                    let mut context_count = 0;
                    for j in hunk_end..max_len.min(hunk_end + 6) {
                        if a_lines.get(j) == b_lines.get(j) {
                            context_count += 1;
                        } else {
                            break;
                        }
                    }
                    if context_count >= 3 {
                        break;
                    }
                }
                hunk_end += 1;
            }

            hunk_end = hunk_end.min(max_len);
            let hunk_context_end = (hunk_end + 3).min(max_len);

            // Emit hunk header
            let a_start = hunk_start + 1;
            let a_count = (hunk_end - hunk_start).min(a_lines.len().saturating_sub(hunk_start));
            let b_count = (hunk_context_end - hunk_start).min(b_lines.len().saturating_sub(hunk_start));

            writeln!(output, "@@ -{},{} +{},{} @@", a_start, a_count, a_start, b_count).ok();

            // Emit lines
            for j in hunk_start..hunk_context_end {
                let a_l = a_lines.get(j).copied();
                let b_l = b_lines.get(j).copied();

                match (a_l, b_l) {
                    (Some(a), Some(b)) if a == b => {
                        writeln!(output, " {}", a).ok();
                    }
                    (Some(a), Some(b)) => {
                        writeln!(output, "-{}", a).ok();
                        writeln!(output, "+{}", b).ok();
                    }
                    (Some(a), None) => {
                        writeln!(output, "-{}", a).ok();
                    }
                    (None, Some(b)) => {
                        writeln!(output, "+{}", b).ok();
                    }
                    (None, None) => {}
                }
            }

            i = hunk_context_end;
        } else {
            i += 1;
        }
    }

    output
}

/// Apply ANSI colors to diff output
fn colorize_diff(diff: &str) -> String {
    let mut output = String::new();

    for line in diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            output.push_str(&format!("\x1b[32m{}\x1b[0m\n", line)); // Green
        } else if line.starts_with('-') && !line.starts_with("---") {
            output.push_str(&format!("\x1b[31m{}\x1b[0m\n", line)); // Red
        } else if line.starts_with("@@") {
            output.push_str(&format!("\x1b[36m{}\x1b[0m\n", line)); // Cyan
        } else if line.starts_with("diff ") {
            output.push_str(&format!("\x1b[1m{}\x1b[0m\n", line)); // Bold
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

/// Output text through a pager (less)
fn output_with_pager(text: &str) -> Result<()> {
    let pager = std::env::var("PAGER").unwrap_or_else(|_| "less".to_string());

    let mut child = Command::new(&pager)
        .args(["-R", "-F", "-X"]) // -R: ANSI colors, -F: quit if one screen, -X: don't clear screen
        .stdin(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| {
            // Fallback: just print directly
            print!("{}", text);
            std::process::exit(0);
        });

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).ok();
    }

    child.wait()?;
    Ok(())
}

/// Color output option
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorOption {
    Auto,
    Always,
    Never,
}

impl std::str::FromStr for ColorOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(ColorOption::Auto),
            "always" => Ok(ColorOption::Always),
            "never" => Ok(ColorOption::Never),
            _ => Err(format!("Invalid color option: {}. Use auto, always, or never.", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_binary() {
        assert!(!is_binary(b"hello world"));
        assert!(is_binary(b"hello\x00world"));
        assert!(!is_binary(b""));
    }

    #[test]
    fn test_colorize_diff() {
        let diff = "+added line\n-removed line\n unchanged\n";
        let colored = colorize_diff(diff);
        assert!(colored.contains("\x1b[32m")); // Green
        assert!(colored.contains("\x1b[31m")); // Red
    }

    #[test]
    fn test_generate_unified_diff_simple() {
        let a = "line1\nline2\nline3\n";
        let b = "line1\nmodified\nline3\n";
        let diff = generate_unified_diff(a, b);
        assert!(diff.contains("@@"));
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+modified"));
    }

    #[test]
    fn test_color_option_parse() {
        assert_eq!("auto".parse::<ColorOption>().unwrap(), ColorOption::Auto);
        assert_eq!("always".parse::<ColorOption>().unwrap(), ColorOption::Always);
        assert_eq!("never".parse::<ColorOption>().unwrap(), ColorOption::Never);
        assert!("invalid".parse::<ColorOption>().is_err());
    }
}
