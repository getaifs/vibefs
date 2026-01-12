use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::io;
use std::path::Path;

use crate::db::MetadataStore;

/// Session information for dashboard display
#[derive(Debug)]
pub struct SessionInfo {
    pub vibe_id: String,
    pub dirty_count: usize,
    pub status: String,
}

/// Run the TUI dashboard
pub async fn run_dashboard<P: AsRef<Path>>(repo_path: P) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let vibe_dir = repo_path.join(".vibe");

    if !vibe_dir.exists() {
        anyhow::bail!("VibeFS not initialized. Run 'vibe init' first.");
    }

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the dashboard
    let result = run_dashboard_loop(&mut terminal, &vibe_dir).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_dashboard_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    vibe_dir: &Path,
) -> Result<()> {
    loop {
        // Collect session information
        let sessions = collect_session_info(vibe_dir)?;

        // Draw the UI
        terminal.draw(|f| {
            let area = f.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(3),
                ])
                .split(area);

            // Title
            let title = Paragraph::new("VibeFS Dashboard - Air Traffic Control")
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, chunks[0]);

            // Session list
            let session_items: Vec<ListItem> = sessions
                .iter()
                .map(|session| {
                    let style = match session.status.as_str() {
                        "active" => Style::default().fg(Color::Green),
                        "promoted" => Style::default().fg(Color::Yellow),
                        _ => Style::default().fg(Color::White),
                    };

                    let content = Line::from(vec![
                        Span::styled(&session.vibe_id, style.add_modifier(Modifier::BOLD)),
                        Span::raw(" | "),
                        Span::raw(format!("{} dirty files", session.dirty_count)),
                        Span::raw(" | "),
                        Span::styled(&session.status, style),
                    ]);

                    ListItem::new(content)
                })
                .collect();

            let session_list = List::new(session_items)
                .block(
                    Block::default()
                        .title("Active Vibe Sessions")
                        .borders(Borders::ALL),
                );
            f.render_widget(session_list, chunks[1]);

            // Help text
            let help = Paragraph::new("Press 'q' to quit | 'r' to refresh")
                .style(Style::default().fg(Color::Gray))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(help, chunks[2]);
        })?;

        // Handle input
        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('r') => continue,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn collect_session_info(vibe_dir: &Path) -> Result<Vec<SessionInfo>> {
    let mut sessions = Vec::new();
    let sessions_dir = vibe_dir.join("sessions");

    if !sessions_dir.exists() {
        return Ok(sessions);
    }

    let metadata_path = vibe_dir.join("metadata.db");
    let metadata = MetadataStore::open(&metadata_path)?;

    for entry in std::fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip JSON files (spawn info)
        if path.extension().map_or(false, |ext| ext == "json") {
            continue;
        }

        if path.is_dir() {
            let vibe_id = path.file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();

            // Check if it's a snapshot
            if vibe_id.contains("_snapshot_") {
                continue;
            }

            // Get dirty count
            let dirty_paths = metadata.get_dirty_paths()?;
            let dirty_count = dirty_paths.len();

            // Determine status
            let status = if vibe_dir.join(format!("refs/vibes/{}", vibe_id)).exists() {
                "promoted".to_string()
            } else {
                "active".to_string()
            };

            sessions.push(SessionInfo {
                vibe_id,
                dirty_count,
                status,
            });
        }
    }

    Ok(sessions)
}
