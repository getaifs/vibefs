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

/// View mode for the dashboard
#[derive(Debug, Clone, PartialEq)]
enum ViewMode {
    List,
    FilePopup { show_excluded: bool },
    DiffPreview,
    ConfirmPromote,
    ConfirmClose,
}

/// Dashboard application state
struct DashboardApp {
    sessions: Vec<SessionInfo>,
    list_state: ListState,
    repo_name: String,
    repo_path: PathBuf,
    view_mode: ViewMode,
    popup_scroll: usize,
    diff_content: Vec<String>,
    diff_scroll: usize,
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
            view_mode: ViewMode::List,
            popup_scroll: 0,
            diff_content: Vec::new(),
            diff_scroll: 0,
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

    fn popup_scroll_down(&mut self, visible_height: usize) {
        let max_items = match &self.view_mode {
            ViewMode::FilePopup { show_excluded } => {
                if let Some(session) = self.selected_session() {
                    if *show_excluded {
                        session.files.excluded.len()
                    } else {
                        session.files.promotable.len()
                    }
                } else {
                    0
                }
            }
            ViewMode::DiffPreview => self.diff_content.len(),
            _ => 0,
        };
        let max_scroll = max_items.saturating_sub(visible_height);
        if self.popup_scroll < max_scroll {
            self.popup_scroll += 1;
        }
    }

    fn popup_scroll_up(&mut self) {
        if self.popup_scroll > 0 {
            self.popup_scroll -= 1;
        }
    }

    fn reset_popup(&mut self) {
        self.view_mode = ViewMode::List;
        self.popup_scroll = 0;
        self.diff_content.clear();
        self.diff_scroll = 0;
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

            // Calculate message bar height (1 if message, 0 otherwise)
            let msg_height = if app.message.is_some() { 1 } else { 0 };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),           // Title
                    Constraint::Min(10),             // Session list
                    Constraint::Length(10),          // Details panel
                    Constraint::Length(msg_height),  // Message bar (dynamic)
                    Constraint::Length(3),           // Help bar (always visible)
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
            } else if app.sessions.is_empty() {
                // Empty state with helpful hint
                Paragraph::new(vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("No sessions yet.", Style::default().fg(Color::DarkGray)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                        Span::styled("n", Style::default().fg(Color::Yellow)),
                        Span::styled(" to create one, or run ", Style::default().fg(Color::DarkGray)),
                        Span::styled("vibe new", Style::default().fg(Color::Cyan)),
                    ]),
                ])
            } else {
                Paragraph::new("No session selected")
            };

            let details_block = details.block(
                Block::default()
                    .title("Details")
                    .borders(Borders::ALL),
            );
            f.render_widget(details_block, chunks[2]);

            // Message bar (separate from help, above it)
            if let Some(ref msg) = app.message {
                let msg_style = if msg.is_error {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Green)
                };
                let msg_widget = Paragraph::new(Span::styled(&msg.text, msg_style))
                    .style(Style::default().bg(Color::DarkGray));
                f.render_widget(msg_widget, chunks[3]);
            }

            // Help bar - always visible with shortcuts
            let help_content = Line::from(vec![
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(":quit "),
                Span::styled("j/k", Style::default().fg(Color::Yellow)),
                Span::raw(":nav "),
                Span::styled("n", Style::default().fg(Color::Yellow)),
                Span::raw(":new "),
                Span::styled("d", Style::default().fg(Color::Yellow)),
                Span::raw(":files "),
                Span::styled("D", Style::default().fg(Color::Yellow)),
                Span::raw(":diff "),
                Span::styled("p", Style::default().fg(Color::Yellow)),
                Span::raw(":promote "),
                Span::styled("s", Style::default().fg(Color::Yellow)),
                Span::raw(":save "),
                Span::styled("c", Style::default().fg(Color::Yellow)),
                Span::raw(":close "),
                Span::styled("r", Style::default().fg(Color::Yellow)),
                Span::raw(":refresh"),
            ]);

            let help = Paragraph::new(help_content)
                .block(Block::default().borders(Borders::ALL).title("Commands"));
            f.render_widget(help, chunks[4]);

            // Popups based on view mode
            match &app.view_mode {
                ViewMode::FilePopup { show_excluded } => {
                    if let Some(session) = app.selected_session() {
                        let popup_area = centered_rect(70, 60, area);
                        f.render_widget(Clear, popup_area);

                        let files_to_show = if *show_excluded {
                            &session.files.excluded
                        } else {
                            &session.files.promotable
                        };

                        let title = if *show_excluded {
                            format!("Excluded Files - {} ({} files) [ESC:close e:promotable j/k:scroll]",
                                session.vibe_id, files_to_show.len())
                        } else {
                            format!("Promotable Files - {} ({} files) [ESC:close e:excluded j/k:scroll]",
                                session.vibe_id, files_to_show.len())
                        };

                        let visible_height = popup_area.height.saturating_sub(2) as usize;
                        let max_scroll = files_to_show.len().saturating_sub(visible_height);
                        let scroll = app.popup_scroll.min(max_scroll);

                        let file_items: Vec<ListItem> = files_to_show
                            .iter()
                            .skip(scroll)
                            .take(visible_height)
                            .map(|file| {
                                let style = if *show_excluded {
                                    Style::default().fg(Color::DarkGray)
                                } else {
                                    Style::default().fg(Color::White)
                                };
                                ListItem::new(Line::from(Span::styled(file.as_str(), style)))
                            })
                            .collect();

                        let file_list = List::new(file_items).block(
                            Block::default()
                                .title(title)
                                .borders(Borders::ALL)
                                .style(Style::default().bg(Color::Black)),
                        );
                        f.render_widget(file_list, popup_area);

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
                ViewMode::DiffPreview => {
                    let popup_area = centered_rect(85, 80, area);
                    f.render_widget(Clear, popup_area);

                    let visible_height = popup_area.height.saturating_sub(2) as usize;
                    let scroll = app.popup_scroll;

                    let diff_lines: Vec<Line> = app.diff_content
                        .iter()
                        .skip(scroll)
                        .take(visible_height)
                        .map(|line| {
                            let style = if line.starts_with('+') && !line.starts_with("+++") {
                                Style::default().fg(Color::Green)
                            } else if line.starts_with('-') && !line.starts_with("---") {
                                Style::default().fg(Color::Red)
                            } else if line.starts_with("@@") {
                                Style::default().fg(Color::Cyan)
                            } else if line.starts_with("diff ") || line.starts_with("index ") {
                                Style::default().fg(Color::Yellow)
                            } else {
                                Style::default().fg(Color::White)
                            };
                            Line::from(Span::styled(line.as_str(), style))
                        })
                        .collect();

                    let session_name = app.selected_session()
                        .map(|s| s.vibe_id.as_str())
                        .unwrap_or("unknown");

                    let title = format!("Diff Preview - {} [ESC:close j/k:scroll]", session_name);

                    let diff_widget = Paragraph::new(diff_lines)
                        .block(Block::default()
                            .title(title)
                            .borders(Borders::ALL)
                            .style(Style::default().bg(Color::Black)));
                    f.render_widget(diff_widget, popup_area);

                    if app.diff_content.len() > visible_height {
                        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                            .begin_symbol(Some("↑"))
                            .end_symbol(Some("↓"));
                        let mut scrollbar_state = ScrollbarState::new(app.diff_content.len())
                            .position(scroll);
                        f.render_stateful_widget(
                            scrollbar,
                            popup_area.inner(&ratatui::layout::Margin { vertical: 1, horizontal: 0 }),
                            &mut scrollbar_state,
                        );
                    }
                }
                ViewMode::ConfirmPromote => {
                    if let Some(session) = app.selected_session() {
                        let popup_area = centered_rect(50, 20, area);
                        f.render_widget(Clear, popup_area);

                        let content = vec![
                            Line::from(""),
                            Line::from(vec![
                                Span::raw("Promote "),
                                Span::styled(&session.vibe_id, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                                Span::raw("?"),
                            ]),
                            Line::from(""),
                            Line::from(vec![
                                Span::styled(
                                    format!("{} files will be committed", session.files.promotable.len()),
                                    Style::default().fg(Color::Gray),
                                ),
                            ]),
                            Line::from(""),
                            Line::from(vec![
                                Span::styled("y", Style::default().fg(Color::Green)),
                                Span::raw(":confirm  "),
                                Span::styled("n/ESC", Style::default().fg(Color::Red)),
                                Span::raw(":cancel"),
                            ]),
                        ];

                        let confirm = Paragraph::new(content)
                            .alignment(ratatui::layout::Alignment::Center)
                            .block(Block::default()
                                .title("Confirm Promote")
                                .borders(Borders::ALL)
                                .style(Style::default().bg(Color::Black)));
                        f.render_widget(confirm, popup_area);
                    }
                }
                ViewMode::ConfirmClose => {
                    if let Some(session) = app.selected_session() {
                        let popup_area = centered_rect(50, 20, area);
                        f.render_widget(Clear, popup_area);

                        let warning = if !session.files.promotable.is_empty() {
                            format!("Warning: {} uncommitted files will be lost!", session.files.promotable.len())
                        } else {
                            "Session is clean.".to_string()
                        };

                        let content = vec![
                            Line::from(""),
                            Line::from(vec![
                                Span::raw("Close session "),
                                Span::styled(&session.vibe_id, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                                Span::raw("?"),
                            ]),
                            Line::from(""),
                            Line::from(vec![
                                Span::styled(
                                    warning,
                                    if session.files.promotable.is_empty() {
                                        Style::default().fg(Color::Gray)
                                    } else {
                                        Style::default().fg(Color::Red)
                                    },
                                ),
                            ]),
                            Line::from(""),
                            Line::from(vec![
                                Span::styled("y", Style::default().fg(Color::Green)),
                                Span::raw(":confirm  "),
                                Span::styled("n/ESC", Style::default().fg(Color::Red)),
                                Span::raw(":cancel"),
                            ]),
                        ];

                        let confirm = Paragraph::new(content)
                            .alignment(ratatui::layout::Alignment::Center)
                            .block(Block::default()
                                .title("Confirm Close")
                                .borders(Borders::ALL)
                                .style(Style::default().bg(Color::Black)));
                        f.render_widget(confirm, popup_area);
                    }
                }
                ViewMode::List => {}
            }
        })?;

        // Handle input with 200ms poll for responsive UI
        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                // Handle popup modes
                match &app.view_mode {
                    ViewMode::FilePopup { show_excluded } => {
                        let show_excluded = *show_excluded;
                        match key.code {
                            KeyCode::Esc => app.reset_popup(),
                            KeyCode::Char('d') if !show_excluded => app.reset_popup(),
                            KeyCode::Char('e') => {
                                app.view_mode = ViewMode::FilePopup { show_excluded: !show_excluded };
                                app.popup_scroll = 0;
                            }
                            KeyCode::Char('j') | KeyCode::Down => app.popup_scroll_down(20),
                            KeyCode::Char('k') | KeyCode::Up => app.popup_scroll_up(),
                            _ => {}
                        }
                        continue;
                    }
                    ViewMode::DiffPreview => {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => app.reset_popup(),
                            KeyCode::Char('j') | KeyCode::Down => app.popup_scroll_down(20),
                            KeyCode::Char('k') | KeyCode::Up => app.popup_scroll_up(),
                            KeyCode::Char('G') => {
                                // Jump to end
                                app.popup_scroll = app.diff_content.len().saturating_sub(20);
                            }
                            KeyCode::Char('g') => {
                                // Jump to start
                                app.popup_scroll = 0;
                            }
                            _ => {}
                        }
                        continue;
                    }
                    ViewMode::ConfirmPromote => {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                if let Some(session) = app.selected_session() {
                                    let vibe_id = session.vibe_id.clone();
                                    let repo_path = app.repo_path.clone();

                                    app.set_message(format!("Promoting {}...", vibe_id), false);
                                    app.reset_popup();

                                    tokio::spawn(async move {
                                        let _ = commands::promote::promote(&repo_path, &vibe_id, None, None).await;
                                    });
                                }
                            }
                            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                                app.reset_popup();
                            }
                            _ => {}
                        }
                        continue;
                    }
                    ViewMode::ConfirmClose => {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                if let Some(session) = app.selected_session() {
                                    let vibe_id = session.vibe_id.clone();
                                    let repo_path = app.repo_path.clone();

                                    app.set_message(format!("Closing {}...", vibe_id), false);
                                    app.reset_popup();

                                    tokio::spawn(async move {
                                        let _ = commands::close::close(&repo_path, &vibe_id, true, false).await;
                                    });
                                }
                            }
                            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                                app.reset_popup();
                            }
                            _ => {}
                        }
                        continue;
                    }
                    ViewMode::List => {}
                }

                // Main list view key handling
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('r') => {
                        app.last_refresh = Instant::now() - std::time::Duration::from_secs(10);
                        app.set_message("Refreshing...".to_string(), false);
                    }
                    KeyCode::Char('j') | KeyCode::Down => app.next(),
                    KeyCode::Char('k') | KeyCode::Up => app.previous(),
                    KeyCode::Char('n') => {
                        // Spawn new session
                        app.set_message("Creating new session... Exit TUI to enter shell.".to_string(), false);
                        let repo_path = app.repo_path.clone();
                        let sessions_dir = repo_path.join(".vibe/sessions");
                        let session_name = crate::names::generate_unique_name(&sessions_dir);

                        tokio::spawn(async move {
                            let _ = commands::spawn::spawn(&repo_path, &session_name).await;
                        });
                    }
                    KeyCode::Char('d') => {
                        if let Some(session) = app.selected_session() {
                            if !session.files.promotable.is_empty() {
                                app.view_mode = ViewMode::FilePopup { show_excluded: false };
                                app.popup_scroll = 0;
                            } else if !session.files.excluded.is_empty() {
                                app.set_message("No promotable files. Press 'e' to view excluded.".to_string(), false);
                            }
                        }
                    }
                    KeyCode::Char('e') => {
                        if let Some(session) = app.selected_session() {
                            if !session.files.excluded.is_empty() {
                                app.view_mode = ViewMode::FilePopup { show_excluded: true };
                                app.popup_scroll = 0;
                            } else {
                                app.set_message("No excluded files.".to_string(), false);
                            }
                        }
                    }
                    KeyCode::Char('D') => {
                        // Show diff preview
                        if let Some(session) = app.selected_session() {
                            if session.files.is_empty() {
                                app.set_message("No changes to diff.".to_string(), false);
                            } else {
                                let vibe_id = session.vibe_id.clone();
                                let repo_path = app.repo_path.clone();

                                // Load diff content synchronously (it's fast for session files)
                                match load_session_diff(&repo_path, &vibe_id) {
                                    Ok(diff_lines) => {
                                        if diff_lines.is_empty() {
                                            app.set_message("No diff available.".to_string(), false);
                                        } else {
                                            app.diff_content = diff_lines;
                                            app.popup_scroll = 0;
                                            app.view_mode = ViewMode::DiffPreview;
                                        }
                                    }
                                    Err(e) => {
                                        app.set_message(format!("Failed to load diff: {}", e), true);
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('c') => {
                        // Show close confirmation
                        if app.selected_session().is_some() {
                            app.view_mode = ViewMode::ConfirmClose;
                        }
                    }
                    KeyCode::Char('p') => {
                        // Show promote confirmation
                        if let Some(session) = app.selected_session() {
                            if session.files.promotable.is_empty() {
                                app.set_message("No files to promote.".to_string(), false);
                            } else {
                                app.view_mode = ViewMode::ConfirmPromote;
                            }
                        }
                    }
                    KeyCode::Char('s') => {
                        // Save checkpoint
                        if let Some(session) = app.selected_session() {
                            let vibe_id = session.vibe_id.clone();
                            let repo_path = app.repo_path.clone();

                            app.set_message(format!("Saving checkpoint for {}...", vibe_id), false);

                            tokio::spawn(async move {
                                let _ = commands::snapshot::snapshot(&repo_path, &vibe_id).await;
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

/// Load diff content for a session
fn load_session_diff(repo_path: &Path, vibe_id: &str) -> Result<Vec<String>> {
    use std::process::Command;

    let session_dir = repo_path.join(".vibe/sessions").join(vibe_id);
    if !session_dir.exists() {
        return Ok(Vec::new());
    }

    let mut diff_lines = Vec::new();

    // Collect all files in session
    let files = collect_dirty_files(&session_dir);

    for file in files {
        let session_file = session_dir.join(&file);
        let repo_file = repo_path.join(&file);

        // Check if file exists in repo
        if repo_file.exists() {
            // Modified file - show diff
            let output = Command::new("diff")
                .args(["-u", "--label", &format!("a/{}", file), "--label", &format!("b/{}", file)])
                .arg(&repo_file)
                .arg(&session_file)
                .output()?;

            let diff_output = String::from_utf8_lossy(&output.stdout);
            if !diff_output.is_empty() {
                for line in diff_output.lines() {
                    diff_lines.push(line.to_string());
                }
                diff_lines.push(String::new());
            }
        } else {
            // New file - show as all additions
            diff_lines.push(format!("diff --git a/{} b/{}", file, file));
            diff_lines.push("new file".to_string());
            diff_lines.push(format!("--- /dev/null"));
            diff_lines.push(format!("+++ b/{}", file));

            if let Ok(content) = std::fs::read_to_string(&session_file) {
                let lines: Vec<&str> = content.lines().collect();
                diff_lines.push(format!("@@ -0,0 +1,{} @@", lines.len()));
                for line in lines {
                    diff_lines.push(format!("+{}", line));
                }
            }
            diff_lines.push(String::new());
        }
    }

    Ok(diff_lines)
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
