//! TUI for browsing trending events

use polymarket_bot::gamma::Event;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

pub struct TrendingAppState {
    pub events: Vec<Event>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub should_quit: bool,
    pub show_details: bool,
}

impl TrendingAppState {
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            events,
            selected_index: 0,
            scroll_offset: 0,
            should_quit: false,
            show_details: false,
        }
    }

    pub fn selected_event(&self) -> Option<&Event> {
        self.events.get(self.selected_index)
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            // Adjust scroll if needed
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index < self.events.len().saturating_sub(1) {
            self.selected_index += 1;
            // Adjust scroll if needed
            let visible_height = 20; // Approximate visible items
            if self.selected_index >= self.scroll_offset + visible_height {
                self.scroll_offset = self.selected_index - visible_height + 1;
            }
        }
    }

    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }
}

pub fn render(f: &mut Frame, app: &TrendingAppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Footer
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new(vec![
        Line::from("🔥 Trending Events".fg(Color::Yellow).bold()),
        Line::from(format!(
            "Showing {} events | Use ↑↓ to navigate | Enter to view details | 'q' to quit",
            app.events.len()
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title("Polymarket"))
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: true });
    f.render_widget(header, chunks[0]);

    // Main content
    if app.show_details {
        render_details(f, app, chunks[1]);
    } else {
        render_list(f, app, chunks[1]);
    }

    // Footer
    let footer_text = if app.show_details {
        "Press 'q' to quit | 'Esc' to go back | 'Enter' to watch this event"
    } else {
        "Press 'q' to quit | ↑↓ to navigate | Enter for details | 'w' to watch event"
    };
    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[2]);
}

fn render_list(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let items: Vec<ListItem> = app
        .events
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(area.height as usize - 2)
        .map(|(idx, event)| {
            let is_selected = idx == app.selected_index;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(Color::White)
            };

            let markets_count = event.markets.len();
            let title = truncate(&event.title, 60);
            let slug = truncate(&event.slug, 40);

            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}. ", idx + 1),
                        Style::default().fg(Color::Gray),
                    ),
                    Span::styled(title, style),
                ]),
                Line::from(vec![
                    Span::styled("  Markets: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        markets_count.to_string(),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(" | Slug: ", Style::default().fg(Color::Gray)),
                    Span::styled(slug, Style::default().fg(Color::Blue)),
                ]),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Trending Events"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        );

    let mut state = ListState::default();
    state.select(Some(app.selected_index.saturating_sub(app.scroll_offset)));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_details(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    if let Some(event) = app.selected_event() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Title: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(&event.title, Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Slug: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(&event.slug, Style::default().fg(Color::Blue)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Event ID: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(&event.id, Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(
                    if event.active {
                        "Active"
                    } else {
                        "Inactive"
                    },
                    Style::default().fg(if event.active { Color::Green } else { Color::Red }),
                ),
                Span::styled(" | ", Style::default().fg(Color::Gray)),
                Span::styled(
                    if event.closed {
                        "Closed"
                    } else {
                        "Open"
                    },
                    Style::default().fg(if event.closed { Color::Red } else { Color::Green }),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Markets: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(
                    event.markets.len().to_string(),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(""),
        ];

        if !event.tags.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Tags: ", Style::default().fg(Color::Yellow).bold()),
            ]));
            for tag in &event.tags {
                lines.push(Line::from(vec![
                    Span::styled("  • ", Style::default().fg(Color::Gray)),
                    Span::styled(&tag.label, Style::default().fg(Color::Cyan)),
                ]));
            }
            lines.push(Line::from(""));
        }

        if !event.markets.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Market Questions:", Style::default().fg(Color::Yellow).bold()),
            ]));
            for (i, market) in event.markets.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {}. ", i + 1),
                        Style::default().fg(Color::Gray),
                    ),
                    Span::styled(truncate(&market.question, 70), Style::default().fg(Color::White)),
                ]));
            }
        }

        let paragraph = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Event Details"))
            .wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

pub async fn run_trending_tui(
    mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
    app_state: Arc<TokioMutex<TrendingAppState>>,
) -> anyhow::Result<Option<String>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};

    loop {
        {
            let app = app_state.lock().await;
            terminal.draw(|f| {
                render(f, &app);
            })?;
        }

        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let mut app = app_state.lock().await;
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            if app.show_details {
                                app.show_details = false;
                            } else {
                                app.should_quit = true;
                                break;
                            }
                        }
                        KeyCode::Up => app.move_up(),
                        KeyCode::Down => app.move_down(),
                        KeyCode::Enter => {
                            if app.show_details {
                                // Return the selected event slug to watch
                                if let Some(event) = app.selected_event() {
                                    return Ok(Some(event.slug.clone()));
                                }
                            } else {
                                app.toggle_details();
                            }
                        }
                        KeyCode::Char('w') => {
                            // Watch the selected event
                            if let Some(event) = app.selected_event() {
                                return Ok(Some(event.slug.clone()));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        {
            let app = app_state.lock().await;
            if app.should_quit {
                break;
            }
        }
    }

    Ok(None)
}

