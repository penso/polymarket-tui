//! TUI for browsing trending events with live trade monitoring

use chrono::DateTime;
use polymarket_bot::gamma::Event;
use polymarket_bot::rtds::RTDSMessage;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

pub struct Trade {
    pub timestamp: i64,
    pub side: String,
    pub outcome: String,
    pub price: f64,
    pub shares: f64,
    pub total_value: f64,
    pub title: String,
    pub user: String,
    pub pseudonym: String,
}

pub struct EventTrades {
    pub trades: Vec<Trade>,
    pub is_watching: bool,
}

impl EventTrades {
    pub fn new() -> Self {
        Self {
            trades: Vec::new(),
            is_watching: false,
        }
    }

    pub fn add_trade(&mut self, msg: &RTDSMessage) {
        let rounded_shares = (msg.payload.size * 100.0).round() / 100.0;
        let total_value = msg.payload.price * msg.payload.size;

        let trade = Trade {
            timestamp: msg.payload.timestamp,
            side: msg.payload.side.clone(),
            outcome: msg.payload.outcome.clone(),
            price: msg.payload.price,
            shares: rounded_shares,
            total_value,
            title: msg.payload.title.clone(),
            user: msg.payload.name.clone(),
            pseudonym: msg.payload.pseudonym.clone(),
        };

        self.trades.insert(0, trade);
        // Keep only the last 500 trades per event
        if self.trades.len() > 500 {
            self.trades.truncate(500);
        }
    }
}

pub struct TrendingAppState {
    pub events: Vec<Event>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub should_quit: bool,
    // Map from event slug to trades
    pub event_trades: HashMap<String, EventTrades>,
    // Map from event slug to websocket task handle
    pub ws_handles: HashMap<String, JoinHandle<()>>,
    // Search functionality
    pub search_mode: bool,
    pub search_query: String,
}

impl TrendingAppState {
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            events,
            selected_index: 0,
            scroll_offset: 0,
            should_quit: false,
            event_trades: HashMap::new(),
            ws_handles: HashMap::new(),
            search_mode: false,
            search_query: String::new(),
        }
    }

    /// Get filtered events based on search query
    pub fn filtered_events(&self) -> Vec<&Event> {
        if self.search_query.is_empty() {
            return self.events.iter().collect();
        }

        let query_lower = self.search_query.to_lowercase();
        self.events
            .iter()
            .filter(|event| {
                event.title.to_lowercase().contains(&query_lower)
                    || event.slug.to_lowercase().contains(&query_lower)
                    || event.tags.iter().any(|tag| tag.label.to_lowercase().contains(&query_lower))
                    || event.markets.iter().any(|market| market.question.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Get the currently selected event from filtered list
    pub fn selected_event_filtered(&self) -> Option<&Event> {
        let filtered = self.filtered_events();
        filtered.get(self.selected_index).copied()
    }

    pub fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
    }

    pub fn exit_search_mode(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn add_search_char(&mut self, c: char) {
        self.search_query.push(c);
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn delete_search_char(&mut self) {
        self.search_query.pop();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn selected_event(&self) -> Option<&Event> {
        if self.search_mode {
            self.selected_event_filtered()
        } else {
            self.events.get(self.selected_index)
        }
    }

    pub fn selected_event_slug(&self) -> Option<String> {
        self.selected_event().map(|e| e.slug.clone())
    }

    pub fn move_up(&mut self) {
        let filtered_len = self.filtered_events().len();
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn move_down(&mut self) {
        let filtered_len = self.filtered_events().len();
        if self.selected_index < filtered_len.saturating_sub(1) {
            self.selected_index += 1;
            let visible_height = 20;
            if self.selected_index >= self.scroll_offset + visible_height {
                self.scroll_offset = self.selected_index - visible_height + 1;
            }
        }
    }

    pub fn is_watching(&self, event_slug: &str) -> bool {
        self.event_trades
            .get(event_slug)
            .map(|et| et.is_watching)
            .unwrap_or(false)
    }

    pub fn get_trades(&self, event_slug: &str) -> &[Trade] {
        self.event_trades
            .get(event_slug)
            .map(|et| et.trades.as_slice())
            .unwrap_or(&[])
    }

    pub fn start_watching(&mut self, event_slug: String, ws_handle: JoinHandle<()>) {
        self.event_trades
            .entry(event_slug.clone())
            .or_insert_with(EventTrades::new)
            .is_watching = true;
        self.ws_handles.insert(event_slug, ws_handle);
    }

    pub fn stop_watching(&mut self, event_slug: &str) {
        if let Some(handle) = self.ws_handles.remove(event_slug) {
            handle.abort();
        }
        if let Some(event_trades) = self.event_trades.get_mut(event_slug) {
            event_trades.is_watching = false;
        }
    }

    pub fn cleanup(&mut self) {
        for handle in self.ws_handles.values() {
            handle.abort();
        }
        self.ws_handles.clear();
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
    let watched_count = app
        .event_trades
        .values()
        .filter(|et| et.is_watching)
        .count();
    let filtered_count = app.filtered_events().len();
    let header_text = if app.search_mode {
        format!(
            "Search: {} | Showing {}/{} events | Watching: {} | Press Esc to exit search",
            app.search_query,
            filtered_count,
            app.events.len(),
            watched_count
        )
    } else {
        format!(
            "Showing {} events | Watching: {} | Press '/' to search | Use ↑↓ to navigate | Enter to watch/unwatch | 'q' to quit",
            filtered_count,
            watched_count
        )
    };
    let header = Paragraph::new(vec![
        Line::from("🔥 Trending Events".fg(Color::Yellow).bold()),
        Line::from(header_text),
    ])
    .block(Block::default().borders(Borders::ALL).title("Polymarket"))
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: true });
    f.render_widget(header, chunks[0]);

    // Main content - split into events list and trades view
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // Events list
            Constraint::Percentage(60), // Trades view
        ])
        .split(chunks[1]);

    render_events_list(f, app, main_chunks[0]);
    render_trades(f, app, main_chunks[1]);

    // Footer
    let footer_text = if app.search_mode {
        "Type to search | Esc to exit search | Enter to watch/unwatch | 'q' to quit"
    } else {
        "Press '/' to search | 'q' to quit | ↑↓ to navigate | Enter to watch/unwatch selected event"
    };
    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[2]);
}

fn render_events_list(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let filtered_events = app.filtered_events();
    let items: Vec<ListItem> = filtered_events
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(area.height as usize - 2)
        .map(|(idx, event)| {
            let is_selected = idx == app.selected_index;
            let is_watching = app.is_watching(&event.slug);
            let trade_count = app.get_trades(&event.slug).len();

            let mut style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(Color::White)
            };

            let title = truncate(&event.title, 45);
            let markets_count = event.markets.len();

            // Add watching indicator
            let watch_indicator = if is_watching {
                "🔴 ".to_string()
            } else {
                "   ".to_string()
            };

            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}.{}", idx + 1, watch_indicator),
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
                    if is_watching {
                        Span::styled(
                            format!(" | Trades: {}", trade_count),
                            Style::default().fg(Color::Green),
                        )
                    } else {
                        Span::styled("", Style::default())
                    },
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

fn render_trades(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    if let Some(event) = app.selected_event() {
        let event_slug = &event.slug;
        let trades = app.get_trades(event_slug);
        let is_watching = app.is_watching(event_slug);

        // Split area into event details and trades
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(12), // Event details
                Constraint::Min(0),     // Trades table
            ])
            .split(area);

        // Render event details
        render_event_details(f, event, is_watching, trades.len(), chunks[0]);

        // Render trades table
        if trades.is_empty() {
            let status_text = if is_watching {
                "Watching for trades... (Press Enter to stop)"
            } else {
                "Not watching. Press Enter to start watching this event."
            };
            let paragraph = Paragraph::new(status_text)
                .block(Block::default().borders(Borders::ALL).title("Trades"))
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
            f.render_widget(paragraph, chunks[1]);
        } else {
            let rows: Vec<Row> = trades
                .iter()
                .take((chunks[1].height as usize).saturating_sub(3))
                .map(|trade| {
                    let time = DateTime::from_timestamp(trade.timestamp, 0)
                        .map(|dt| dt.format("%H:%M:%S").to_string())
                        .unwrap_or_else(|| "now".to_string());

                    let side_style = if trade.side == "BUY" {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    };

                    let outcome_style = if trade.outcome == "Yes" {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    };

                    let title_truncated = truncate(&trade.title, 30);
                    let user_truncated = truncate(&trade.user, 15);
                    let side_text = if trade.side == "BUY" {
                        "🟢 BUY".to_string()
                    } else {
                        "🔴 SELL".to_string()
                    };
                    let outcome_text = trade.outcome.clone();

                    Row::new(vec![
                        Cell::from(time).style(Style::default().fg(Color::Gray)),
                        Cell::from(side_text).style(side_style),
                        Cell::from(outcome_text).style(outcome_style),
                        Cell::from(format!("${:.4}", trade.price)),
                        Cell::from(format!("{:.2}", trade.shares)),
                        Cell::from(format!("${:.2}", trade.total_value)),
                        Cell::from(title_truncated),
                        Cell::from(user_truncated),
                    ])
                })
                .collect();

            let table = Table::new(
                rows,
                [
                    Constraint::Length(10), // Time
                    Constraint::Length(8),  // Side
                    Constraint::Length(5),  // Outcome
                    Constraint::Length(10), // Price
                    Constraint::Length(10), // Shares
                    Constraint::Length(10), // Value
                    Constraint::Percentage(30), // Title
                    Constraint::Length(15), // User
                ],
            )
            .header(
                Row::new(vec![
                    "Time", "Side", "Out", "Price", "Shares", "Value", "Market", "User",
                ])
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(
                        "Trades ({})",
                        if is_watching { "🔴 Watching" } else { "Stopped" }
                    )),
            )
            .column_spacing(1);

            f.render_widget(table, chunks[1]);
        }
    } else {
        let paragraph = Paragraph::new("No event selected")
            .block(Block::default().borders(Borders::ALL).title("Event Details & Trades"))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(paragraph, area);
    }
}

fn render_event_details(f: &mut Frame, event: &Event, is_watching: bool, trade_count: usize, area: Rect) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Title: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(truncate(&event.title, 60), Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Slug: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(truncate(&event.slug, 60), Style::default().fg(Color::Blue)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Event ID: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(truncate(&event.id, 50), Style::default().fg(Color::White)),
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
            Span::styled(" | ", Style::default().fg(Color::Gray)),
            Span::styled(
                if is_watching {
                    "🔴 Watching"
                } else {
                    "Not Watching"
                },
                Style::default().fg(if is_watching { Color::Red } else { Color::Gray }),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Markets: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(
                event.markets.len().to_string(),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(" | ", Style::default().fg(Color::Gray)),
            Span::styled("Trades: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(
                trade_count.to_string(),
                Style::default().fg(if is_watching { Color::Green } else { Color::Gray }),
            ),
        ]),
    ];

    if !event.tags.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Tags: ", Style::default().fg(Color::Yellow).bold()),
        ]));
        for tag in &event.tags {
            lines.push(Line::from(vec![
                Span::styled("  • ", Style::default().fg(Color::Gray)),
                Span::styled(truncate(&tag.label, 50), Style::default().fg(Color::Cyan)),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Event Details"))
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
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
    use polymarket_bot::RTDSClient;

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
                        KeyCode::Char('q') => {
                            if app.search_mode {
                                app.exit_search_mode();
                            } else {
                                app.should_quit = true;
                                break;
                            }
                        }
                        KeyCode::Esc => {
                            if app.search_mode {
                                app.exit_search_mode();
                            } else {
                                app.should_quit = true;
                                break;
                            }
                        }
                        KeyCode::Char('/') => {
                            if !app.search_mode {
                                app.enter_search_mode();
                            }
                        }
                        KeyCode::Up => {
                            if !app.search_mode {
                                app.move_up();
                            }
                        }
                        KeyCode::Down => {
                            if !app.search_mode {
                                app.move_down();
                            }
                        }
                        KeyCode::Backspace => {
                            if app.search_mode {
                                app.delete_search_char();
                            }
                        }
                        KeyCode::Enter => {
                            if app.search_mode {
                                // Exit search mode and keep selection
                                app.search_mode = false;
                            } else {
                                // Toggle watching the selected event
                                if let Some(event_slug) = app.selected_event_slug() {
                                    if app.is_watching(&event_slug) {
                                        // Stop watching
                                        app.stop_watching(&event_slug);
                                    } else {
                                        // Start watching
                                        let event_slug_clone = event_slug.clone();

                                        // Ensure the event_trades entry exists before starting websocket
                                        app.event_trades
                                            .entry(event_slug_clone.clone())
                                            .or_insert_with(EventTrades::new);

                                        let app_state_ws = Arc::clone(&app_state);
                                        let event_slug_for_closure = event_slug_clone.clone();

                                        let rtds_client = RTDSClient::new().with_event_slug(event_slug_clone.clone());
                                        let ws_handle = tokio::spawn(async move {
                                            let _ = rtds_client
                                                .connect_and_listen(move |msg| {
                                                    let app_state = Arc::clone(&app_state_ws);
                                                    let event_slug = event_slug_for_closure.clone();
                                                    tokio::spawn(async move {
                                                        let mut app = app_state.lock().await;
                                                        if let Some(event_trades) = app.event_trades.get_mut(&event_slug) {
                                                            event_trades.add_trade(&msg);
                                                        }
                                                    });
                                                })
                                                .await;
                                        });

                                        app.start_watching(event_slug_clone, ws_handle);
                                    }
                                }
                            }
                        }
                        KeyCode::Char(c) => {
                            if app.search_mode {
                                app.add_search_char(c);
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

    // Cleanup
    {
        let mut app = app_state.lock().await;
        app.cleanup();
    }

    Ok(None)
}
