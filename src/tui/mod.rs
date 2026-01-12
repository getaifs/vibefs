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
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::io;
use std::path::{Path, PathBuf};

use crate::commands;

/// Session information for dashboard display
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub vibe_id: String,
    pub repo_name: String,
    pub repo_path: String,
    pub dirty_files: Vec<String>,
    pub status: String,
    pub mount_point: Option<String>,
}

/// Dashboard application state
struct DashboardApp {
    sessions: Vec<SessionInfo>,
    list_state: ListState,
    repo_name: String,
    repo_path: String,
    show_dirty_popup: bool,
    message: Option<(String, bool)>, // (message, is_error)
}

impl DashboardApp {
    fn new(repo_name: String, repo_path: String) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            sessions: Vec::new(),
            list_state,
            repo_name,
            repo_path,
            show_dirty_popup: false,
            message: None,
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
    
    let repo_path_str = repo_path.to_string_lossy().to_string();

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the dashboard
    let result = run_dashboard_loop(&mut terminal, &vibe_dir, repo_name, repo_path_str).await;

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
    repo_path: String,
) -> Result<()> {
    let mut app = DashboardApp::new(repo_name, repo_path);

    loop {
        // Collect session information
        app.sessions = collect_session_info(vibe_dir, &app.repo_name, &app.repo_path)?;

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
                    Constraint::Length(9),  // Details panel (increased for repo path)
                    Constraint::Length(3),  // Help
                ])
                .split(area);

            // Title with repo path
            let title = Paragraph::new(format!(
                "VibeFS Dashboard - {} - Air Traffic Control (Current Repo)",
                app.repo_path
            ))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, chunks[0]);

            // Session list
            let session_items: Vec<ListItem> = app
                .sessions
                .iter()
                .map(|session| {
                    let style = match session.status.as_str() {
                        "mounted" => Style::default().fg(Color::Green),
                        "promoted" => Style::default().fg(Color::Yellow),
                        "unmounted" => Style::default().fg(Color::Gray),
                        _ => Style::default().fg(Color::White),
                    };

                    let dirty_indicator = if session.dirty_files.is_empty() {
                        Span::styled(" ", Style::default())
                    } else {
                        Span::styled(
                            format!(" [{}]", session.dirty_files.len()),
                            Style::default().fg(Color::Red),
                        )
                    };

                    let content = Line::from(vec![
                        Span::styled(
                            format!("{:<20}", session.vibe_id),
                            style.add_modifier(Modifier::BOLD),
                        ),
                        dirty_indicator,
                        Span::raw("  "),
                        Span::styled(
                            format!("{:<10}", session.status),
                            style,
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
                .highlight_symbol("> ");
            f.render_stateful_widget(session_list, chunks[1], &mut app.list_state);

            // Details panel
            let details = if let Some(session) = app.selected_session() {
                let mut lines = vec![
                    Line::from(vec![
                        Span::styled("Session:    ", Style::default().fg(Color::Gray)),
                        Span::styled(&session.vibe_id, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(vec![
                        Span::styled("Repository: ", Style::default().fg(Color::Gray)),
                        Span::styled(&session.repo_path, Style::default().fg(Color::White)),
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

                if session.dirty_files.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("Dirty files: ", Style::default().fg(Color::Gray)),
                        Span::styled("none", Style::default().fg(Color::Green)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("Dirty files: ", Style::default().fg(Color::Gray)),
                        Span::styled(
                            format!("{} (press 'd' to view)", session.dirty_files.len()),
                            Style::default().fg(Color::Red),
                        ),
                    ]));
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

            // Help text
            let help_text = if app.message.is_some() {
                let (msg, is_error) = app.message.as_ref().unwrap();
                Span::styled(
                    msg,
                    if *is_error {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::Green)
                    },
                )
            } else {
                Span::styled(
                    "q:quit | j/k:navigate | c:close session | p:promote | d:dirty files | r:refresh",
                    Style::default().fg(Color::Gray),
                )
            };

            let help = Paragraph::new(Line::from(help_text))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(help, chunks[3]);

            // Dirty files popup
            if app.show_dirty_popup {
                if let Some(session) = app.selected_session() {
                    let popup_area = centered_rect(60, 50, area);
                    f.render_widget(Clear, popup_area);

                    let dirty_items: Vec<ListItem> = session
                        .dirty_files
                        .iter()
                        .map(|f| ListItem::new(Line::from(f.as_str())))
                        .collect();

                    let dirty_list = List::new(dirty_items).block(
                        Block::default()
                            .title(format!("Dirty Files - {} (ESC to close)", session.vibe_id))
                            .borders(Borders::ALL)
                            .style(Style::default().bg(Color::Black)),
                    );
                    f.render_widget(dirty_list, popup_area);
                }
            }
        })?;

        // Handle input
        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                // Clear message on any keypress
                app.message = None;

                if app.show_dirty_popup {
                    if key.code == KeyCode::Esc || key.code == KeyCode::Char('d') {
                        app.show_dirty_popup = false;
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('r') => continue,
                    KeyCode::Char('j') | KeyCode::Down => app.next(),
                    KeyCode::Char('k') | KeyCode::Up => app.previous(),
                    KeyCode::Char('d') => {
                        if app.selected_session().is_some() {
                            app.show_dirty_popup = true;
                        }
                    }
                    KeyCode::Char('c') => {
                        // Close session
                        if let Some(session) = app.selected_session() {
                            let vibe_id = session.vibe_id.clone();
                            let repo_path = PathBuf::from(&app.repo_path);
                            
                            app.message = Some((
                                format!("Closing session {}...", vibe_id),
                                false,
                            ));
                            
                            // Spawn task to close session to avoid blocking TUI
                            tokio::spawn(async move {
                                let _ = commands::close::close(&repo_path, &vibe_id, true, false).await;
                            });
                        }
                    }
                    KeyCode::Char('p') => {
                        // Show hint about promote command
                        if let Some(session) = app.selected_session() {
                            app.message = Some((
                                format!("Use: vibe promote {}", session.vibe_id),
                                false,
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn collect_session_info(vibe_dir: &Path, repo_name: &str, repo_path: &str) -> Result<Vec<SessionInfo>> {
    let mut sessions = Vec::new();
    let sessions_dir = vibe_dir.join("sessions");

    if !sessions_dir.exists() {
        return Ok(sessions);
    }

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

            // Get dirty files from session directory
            let dirty_files = collect_dirty_files(&path);

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
                repo_path: repo_path.to_string(),
                dirty_files,
                status,
                mount_point,
            });
        }
    }

    // Sort by vibe_id
    sessions.sort_by(|a, b| a.vibe_id.cmp(&b.vibe_id));

    Ok(sessions)
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