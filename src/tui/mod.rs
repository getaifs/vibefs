use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Terminal,
};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::commands;
use crate::gitignore::PromoteFilter;

/// Categorized file info for display
#[derive(Debug, Clone, Default)]
pub struct FileCategories {
    /// Files that will be promoted (new + modified, not gitignored)
    pub promotable: Vec<String>,
    /// Files that are gitignored (excluded from promotion)
    pub excluded: Vec<String>,
}

impl FileCategories {
    pub fn total(&self) -> usize {
        self.promotable.len() + self.excluded.len()
    }

    pub fn is_empty(&self) -> bool {
        self.promotable.is_empty() && self.excluded.is_empty()
    }
}

/// Session information for dashboard display
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub vibe_id: String,
    pub repo_name: String,
    pub repo_path: String,
    pub files: FileCategories,
    pub status: String,
    pub mount_point: Option<String>,
}

/// Message with timestamp for display
struct Message {
    text: String,
    is_error: bool,
    created_at: Instant,
}

impl Message {
    fn new(text: String, is_error: bool) -> Self {
        Self {
            text,
            is_error,
            created_at: Instant::now(),
        }
    }

    fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() >= 5
    }
}

/// Dashboard application state
struct DashboardApp {
    sessions: Vec<SessionInfo>,
    list_state: ListState,
    repo_name: String,
    repo_path: PathBuf,
    show_dirty_popup: bool,
    popup_scroll: usize,
    popup_show_excluded: bool,
    message: Option<Message>,
    last_refresh: Instant,
}

impl DashboardApp {
    fn new(repo_name: String, repo_path: PathBuf) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            sessions: Vec::new(),
            list_state,
            repo_name,
            repo_path,
            show_dirty_popup: false,
            popup_scroll: 0,
            popup_show_excluded: false,
            message: None,
            last_refresh: Instant::now(),
        }
    }

    fn selected_session(&self) -> Option<&SessionInfo> {
        self.list_state.selected().and_then(|i| self.sessions.get(i))
    }

    fn next(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.sessions.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.sessions.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn set_message(&mut self, text: String, is_error: bool) {
        self.message = Some(Message::new(text, is_error));
    }

    fn clear_expired_message(&mut self) {
        if let Some(ref msg) = self.message {
            if msg.is_expired() {
                self.message = None;
            }
        }
    }

    fn popup_scroll_down(&mut self) {
        if let Some(session) = self.selected_session() {
            let max_scroll = if self.popup_show_excluded {
                session.files.excluded.len().saturating_sub(1)
            } else {
                session.files.promotable.len().saturating_sub(1)
            };
            if self.popup_scroll < max_scroll {
                self.popup_scroll += 1;
            }
        }
    }

    fn popup_scroll_up(&mut self) {
        if self.popup_scroll > 0 {
            self.popup_scroll -= 1;
        }
    }
}

/// Run the TUI dashboard
pub async fn run_dashboard<P: AsRef<Path>>(repo_path: P) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    if !vibe_dir.exists() {
        anyhow::bail!("VibeFS not initialized. Run 'vibe init' first.");
    }

    // Get repo name from the directory
    let repo_name = repo_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the dashboard
    let result = run_dashboard_loop(&mut terminal, &vibe_dir, repo_name, repo_path.to_path_buf()).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_dashboard_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    vibe_dir: &Path,
    repo_name: String,
    repo_path: PathBuf,
) -> Result<()> {
    let mut app = DashboardApp::new(repo_name, repo_path.clone());

    loop {
        // Refresh session info every 2 seconds (instead of 500ms)
        if app.last_refresh.elapsed().as_secs() >= 2 || app.sessions.is_empty() {
            app.sessions = collect_session_info(vibe_dir, &app.repo_name, &repo_path)?;
            app.last_refresh = Instant::now();
        }

        // Clear expired messages
        app.clear_expired_message();

        // Ensure selection is valid
        if !app.sessions.is_empty() {
            if let Some(selected) = app.list_state.selected() {
                if selected >= app.sessions.len() {
                    app.list_state.select(Some(app.sessions.len() - 1));
                }
            } else {
                app.list_state.select(Some(0));
            }
        }

        // Draw the UI
        terminal.draw(|f| {
            let area = f.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),  // Title
                    Constraint::Min(10),    // Session list
                    Constraint::Length(10), // Details panel
                    Constraint::Length(3),  // Help bar (always visible)
                ])
                .split(area);

            // Title with repo path
            let title = Paragraph::new(format!(
                "VibeFS Dashboard - {}",
                app.repo_path.display()
            ))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL).title("Air Traffic Control"));
            f.render_widget(title, chunks[0]);

            // Session list with improved indicators
            let session_items: Vec<ListItem> = app
                .sessions
                .iter()
                .map(|session| {
                    // Determine status color based on state
                    let (status_color, status_icon) = match session.status.as_str() {
                        "mounted" if session.files.promotable.is_empty() => (Color::Green, "●"),
                        "mounted" => (Color::Yellow, "●"),
                        "promoted" => (Color::Blue, "✓"),
                        "unmounted" => (Color::DarkGray, "○"),
                        _ => (Color::White, "?"),
                    };

                    // Show promotable count (what matters) vs total
                    let file_indicator = if session.files.is_empty() {
                        Span::styled("  clean", Style::default().fg(Color::DarkGray))
                    } else if session.files.promotable.is_empty() {
                        // Has files but all excluded
                        Span::styled(
                            format!("  ({} excluded)", session.files.excluded.len()),
                            Style::default().fg(Color::DarkGray),
                        )
                    } else if session.files.excluded.is_empty() {
                        // Only promotable files
                        Span::styled(
                            format!("  {} files", session.files.promotable.len()),
                            Style::default().fg(Color::Yellow),
                        )
                    } else {
                        // Both promotable and excluded
                        Span::styled(
                            format!("  {} files (+{} excluded)",
                                session.files.promotable.len(),
                                session.files.excluded.len()),
                            Style::default().fg(Color::Yellow),
                        )
                    };

                    let content = Line::from(vec![
                        Span::styled(
                            format!("{} ", status_icon),
                            Style::default().fg(status_color),
                        ),
                        Span::styled(
                            format!("{:<20}", session.vibe_id),
                            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
                        ),
                        file_indicator,
                        Span::raw("  "),
                        Span::styled(
                            format!("{:<10}", session.status),
                            Style::default().fg(status_color),
                        ),
                    ]);

                    ListItem::new(content)
                })
                .collect();

            let session_list = List::new(session_items)
                .block(
                    Block::default()
                        .title(format!("Sessions ({})", app.sessions.len()))
                        .borders(Borders::ALL),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ");
            f.render_stateful_widget(session_list, chunks[1], &mut app.list_state);

            // Details panel with categorized file info
            let details = if let Some(session) = app.selected_session() {
                let mut lines = vec![
                    Line::from(vec![
                        Span::styled("Session:    ", Style::default().fg(Color::Gray)),
                        Span::styled(&session.vibe_id, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(vec![
                        Span::styled("Status:     ", Style::default().fg(Color::Gray)),
                        Span::styled(&session.status, Style::default().fg(Color::White)),
                    ]),
                ];

                if let Some(mount) = &session.mount_point {
                    lines.push(Line::from(vec![
                        Span::styled("Mount:      ", Style::default().fg(Color::Gray)),
                        Span::styled(mount, Style::default().fg(Color::White)),
                    ]));
                }

                // File summary with categories
                lines.push(Line::from(""));

                if session.files.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("Files:      ", Style::default().fg(Color::Gray)),
                        Span::styled("No changes", Style::default().fg(Color::Green)),
                    ]));
                } else {
                    // Promotable files
                    if !session.files.promotable.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled("Promotable: ", Style::default().fg(Color::Gray)),
                            Span::styled(
                                format!("{} files", session.files.promotable.len()),
                                Style::default().fg(Color::Yellow),
                            ),
                            Span::styled(" (press 'd' to view)", Style::default().fg(Color::DarkGray)),
                        ]));
                    }

                    // Excluded files
                    if !session.files.excluded.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled("Excluded:   ", Style::default().fg(Color::Gray)),
                            Span::styled(
                                format!("{} files", session.files.excluded.len()),
                                Style::default().fg(Color::DarkGray),
                            ),
                            Span::styled(" (gitignored, press 'e' to view)", Style::default().fg(Color::DarkGray)),
                        ]));
                    }
                }

                Paragraph::new(lines)
            } else {
                Paragraph::new("No session selected")
            };

            let details_block = details.block(
                Block::default()
                    .title("Details")
                    .borders(Borders::ALL),
            );
            f.render_widget(details_block, chunks[2]);

            // Help bar - always visible with shortcuts, or message if present
            let help_content = if let Some(ref msg) = app.message {
                Line::from(vec![
                    Span::styled(
                        &msg.text,
                        if msg.is_error {
                            Style::default().fg(Color::Red)
                        } else {
                            Style::default().fg(Color::Green)
                        },
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled("q", Style::default().fg(Color::Yellow)),
                    Span::raw(":quit "),
                    Span::styled("j/k", Style::default().fg(Color::Yellow)),
                    Span::raw(":nav "),
                    Span::styled("d", Style::default().fg(Color::Yellow)),
                    Span::raw(":files "),
                    Span::styled("e", Style::default().fg(Color::Yellow)),
                    Span::raw(":excluded "),
                    Span::styled("c", Style::default().fg(Color::Yellow)),
                    Span::raw(":close "),
                    Span::styled("p", Style::default().fg(Color::Yellow)),
                    Span::raw(":promote "),
                    Span::styled("r", Style::default().fg(Color::Yellow)),
                    Span::raw(":refresh"),
                ])
            };

            let help = Paragraph::new(help_content)
                .block(Block::default().borders(Borders::ALL).title("Commands"));
            f.render_widget(help, chunks[3]);

            // Dirty files popup (scrollable)
            if app.show_dirty_popup {
                if let Some(session) = app.selected_session() {
                    let popup_area = centered_rect(70, 60, area);
                    f.render_widget(Clear, popup_area);

                    let files_to_show = if app.popup_show_excluded {
                        &session.files.excluded
                    } else {
                        &session.files.promotable
                    };

                    let title = if app.popup_show_excluded {
                        format!("Excluded Files - {} ({} files) [ESC:close e:promotable j/k:scroll]",
                            session.vibe_id, files_to_show.len())
                    } else {
                        format!("Promotable Files - {} ({} files) [ESC:close e:excluded j/k:scroll]",
                            session.vibe_id, files_to_show.len())
                    };

                    // Calculate visible area (popup height minus borders)
                    let visible_height = popup_area.height.saturating_sub(2) as usize;
                    let max_scroll = files_to_show.len().saturating_sub(visible_height);
                    let scroll = app.popup_scroll.min(max_scroll);

                    let file_items: Vec<ListItem> = files_to_show
                        .iter()
                        .skip(scroll)
                        .take(visible_height)
                        .map(|f| {
                            let style = if app.popup_show_excluded {
                                Style::default().fg(Color::DarkGray)
                            } else {
                                Style::default().fg(Color::White)
                            };
                            ListItem::new(Line::from(Span::styled(f.as_str(), style)))
                        })
                        .collect();

                    let file_list = List::new(file_items).block(
                        Block::default()
                            .title(title)
                            .borders(Borders::ALL)
                            .style(Style::default().bg(Color::Black)),
                    );
                    f.render_widget(file_list, popup_area);

                    // Scrollbar if needed
                    if files_to_show.len() > visible_height {
                        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                            .begin_symbol(Some("↑"))
                            .end_symbol(Some("↓"));
                        let mut scrollbar_state = ScrollbarState::new(files_to_show.len())
                            .position(scroll);
                        f.render_stateful_widget(
                            scrollbar,
                            popup_area.inner(&ratatui::layout::Margin { vertical: 1, horizontal: 0 }),
                            &mut scrollbar_state,
                        );
                    }
                }
            }
        })?;

        // Handle input with 200ms poll for responsive UI
        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if app.show_dirty_popup {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('d') if !app.popup_show_excluded => {
                            app.show_dirty_popup = false;
                            app.popup_scroll = 0;
                        }
                        KeyCode::Esc => {
                            app.show_dirty_popup = false;
                            app.popup_scroll = 0;
                        }
                        KeyCode::Char('e') => {
                            app.popup_show_excluded = !app.popup_show_excluded;
                            app.popup_scroll = 0;
                        }
                        KeyCode::Char('j') | KeyCode::Down => app.popup_scroll_down(),
                        KeyCode::Char('k') | KeyCode::Up => app.popup_scroll_up(),
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('r') => {
                        app.last_refresh = Instant::now() - std::time::Duration::from_secs(10);
                        app.set_message("Refreshing...".to_string(), false);
                    }
                    KeyCode::Char('j') | KeyCode::Down => app.next(),
                    KeyCode::Char('k') | KeyCode::Up => app.previous(),
                    KeyCode::Char('d') => {
                        if let Some(session) = app.selected_session() {
                            if !session.files.promotable.is_empty() {
                                app.show_dirty_popup = true;
                                app.popup_show_excluded = false;
                                app.popup_scroll = 0;
                            } else if !session.files.excluded.is_empty() {
                                app.set_message("No promotable files. Press 'e' to view excluded.".to_string(), false);
                            }
                        }
                    }
                    KeyCode::Char('e') => {
                        if let Some(session) = app.selected_session() {
                            if !session.files.excluded.is_empty() {
                                app.show_dirty_popup = true;
                                app.popup_show_excluded = true;
                                app.popup_scroll = 0;
                            } else {
                                app.set_message("No excluded files.".to_string(), false);
                            }
                        }
                    }
                    KeyCode::Char('c') => {
                        // Close session
                        if let Some(session) = app.selected_session() {
                            let vibe_id = session.vibe_id.clone();
                            let repo_path = app.repo_path.clone();

                            app.set_message(format!("Closing session {}...", vibe_id), false);

                            // Spawn task to close session to avoid blocking TUI
                            tokio::spawn(async move {
                                let _ = commands::close::close(&repo_path, &vibe_id, true, false).await;
                            });
                        }
                    }
                    KeyCode::Char('p') => {
                        // Show promote command hint
                        if let Some(session) = app.selected_session() {
                            if session.files.promotable.is_empty() {
                                app.set_message("No files to promote.".to_string(), false);
                            } else {
                                app.set_message(
                                    format!("Run: vibe promote {} ({} files)",
                                        session.vibe_id,
                                        session.files.promotable.len()),
                                    false,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn collect_session_info(vibe_dir: &Path, repo_name: &str, repo_path: &Path) -> Result<Vec<SessionInfo>> {
    let mut sessions = Vec::new();
    let sessions_dir = vibe_dir.join("sessions");

    if !sessions_dir.exists() {
        return Ok(sessions);
    }

    // Create gitignore filter once for all sessions
    let filter = PromoteFilter::new(repo_path, None).ok();

    for entry in std::fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip JSON files (spawn info)
        if path.extension().map_or(false, |ext| ext == "json") {
            continue;
        }

        if path.is_dir() {
            let vibe_id = path.file_name().unwrap().to_string_lossy().to_string();

            // Check if it's a snapshot
            if vibe_id.contains("_snapshot_") {
                continue;
            }

            // Get dirty files and categorize them
            let all_files = collect_dirty_files(&path);
            let files = categorize_files(&all_files, &filter, &path, repo_path);

            // Try to load spawn info to get mount point
            let spawn_info_path = sessions_dir.join(format!("{}.json", vibe_id));
            let mount_point = if spawn_info_path.exists() {
                std::fs::read_to_string(&spawn_info_path)
                    .ok()
                    .and_then(|json| {
                        serde_json::from_str::<serde_json::Value>(&json)
                            .ok()
                            .and_then(|v| v["mount_point"].as_str().map(String::from))
                    })
            } else {
                None
            };

            // Determine status
            let status = if vibe_dir
                .join(format!("refs/vibes/{}", vibe_id))
                .exists()
            {
                "promoted".to_string()
            } else if mount_point.is_some() {
                // Check if mount point actually exists/is mounted
                let mp = mount_point.as_ref().unwrap();
                if std::path::Path::new(mp).exists() {
                    "mounted".to_string()
                } else {
                    "unmounted".to_string()
                }
            } else {
                "active".to_string()
            };

            sessions.push(SessionInfo {
                vibe_id,
                repo_name: repo_name.to_string(),
                repo_path: repo_path.display().to_string(),
                files,
                status,
                mount_point,
            });
        }
    }

    // Sort by vibe_id
    sessions.sort_by(|a, b| a.vibe_id.cmp(&b.vibe_id));

    Ok(sessions)
}

fn categorize_files(
    all_files: &[String],
    filter: &Option<PromoteFilter>,
    session_dir: &Path,
    repo_path: &Path,
) -> FileCategories {
    // Try to load session-specific gitignore if it exists
    let session_filter = PromoteFilter::new(repo_path, Some(session_dir)).ok();
    let active_filter = session_filter.as_ref().or(filter.as_ref());

    match active_filter {
        Some(f) => {
            let (promotable_refs, excluded_refs) = f.partition_paths(all_files);
            FileCategories {
                promotable: promotable_refs.into_iter().cloned().collect(),
                excluded: excluded_refs.into_iter().cloned().collect(),
            }
        }
        None => {
            // No filter available, treat all as promotable
            FileCategories {
                promotable: all_files.to_vec(),
                excluded: Vec::new(),
            }
        }
    }
}

fn collect_dirty_files(session_dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_files_recursive(session_dir, session_dir, &mut files);
    files
}

fn collect_files_recursive(base: &Path, current: &Path, files: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(current) {
        for entry in entries.flatten() {
            let path = entry.path();

            // Skip macOS resource fork files (._filename)
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.starts_with("._") {
                    continue;
                }
                // Also skip .DS_Store
                if name_str == ".DS_Store" {
                    continue;
                }
            }

            if path.is_dir() {
                collect_files_recursive(base, &path, files);
            } else if let Ok(rel) = path.strip_prefix(base) {
                files.push(rel.display().to_string());
            }
        }
    }
}

/// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_categories() {
        let cats = FileCategories {
            promotable: vec!["a.rs".to_string(), "b.rs".to_string()],
            excluded: vec!["node_modules/x.js".to_string()],
        };
        assert_eq!(cats.total(), 3);
        assert!(!cats.is_empty());
    }

    #[test]
    fn test_file_categories_empty() {
        let cats = FileCategories::default();
        assert_eq!(cats.total(), 0);
        assert!(cats.is_empty());
    }

    #[test]
    fn test_message_expiry() {
        let msg = Message::new("test".to_string(), false);
        assert!(!msg.is_expired());
        // Note: Can't easily test expiry without waiting 5 seconds
    }
}
