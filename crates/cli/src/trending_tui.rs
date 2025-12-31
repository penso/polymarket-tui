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
        }
    }

    pub fn selected_event(&self) -> Option<&Event> {
        self.events.get(self.selected_index)
    }

    pub fn selected_event_slug(&self) -> Option<String> {
        self.selected_event().map(|e| e.slug.clone())
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index < self.events.len().saturating_sub(1) {
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
    let header = Paragraph::new(vec![
        Line::from("🔥 Trending Events".fg(Color::Yellow).bold()),
        Line::from(format!(
            "Showing {} events | Watching: {} | Use ↑↓ to navigate | Enter to watch/unwatch | 'q' to quit",
            app.events.len(),
            watched_count
        )),
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
    let footer = Paragraph::new(
        "Press 'q' to quit | ↑↓ to navigate | Enter to watch/unwatch selected event",
    )
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[2]);
}

fn render_events_list(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let items: Vec<ListItem> = app
        .events
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
    if let Some(event_slug) = app.selected_event_slug() {
        let trades = app.get_trades(&event_slug);
        let is_watching = app.is_watching(&event_slug);

        if trades.is_empty() {
            let status_text = if is_watching {
                "Watching for trades... (Press Enter to stop)"
            } else {
                "Not watching. Press Enter to start watching this event."
            };
            let paragraph = Paragraph::new(status_text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Trades - {}", truncate(&event_slug, 50))),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
            f.render_widget(paragraph, area);
        } else {
            let rows: Vec<Row> = trades
                .iter()
                .take((area.height as usize).saturating_sub(3))
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
                        "Trades - {} ({})",
                        truncate(&event_slug, 40),
                        if is_watching { "🔴 Watching" } else { "Stopped" }
                    )),
            )
            .column_spacing(1);

            f.render_widget(table, area);
        }
    } else {
        let paragraph = Paragraph::new("No event selected")
            .block(Block::default().borders(Borders::ALL).title("Trades"))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
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
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.should_quit = true;
                            break;
                        }
                        KeyCode::Up => app.move_up(),
                        KeyCode::Down => app.move_down(),
                        KeyCode::Enter => {
                            // Toggle watching the selected event
                            if let Some(event_slug) = app.selected_event_slug() {
                                if app.is_watching(&event_slug) {
                                    // Stop watching
                                    app.stop_watching(&event_slug);
                                } else {
                                    // Start watching
                                    let event_slug_clone = event_slug.clone();
                                    let app_state_ws = Arc::clone(&app_state);

                                    let rtds_client = RTDSClient::new().with_event_slug(event_slug_clone.clone());
                                    let ws_handle = tokio::spawn(async move {
                                        let _ = rtds_client
                                            .connect_and_listen(move |msg| {
                                                let app_state = Arc::clone(&app_state_ws);
                                                tokio::spawn(async move {
                                                    let mut app = app_state.lock().await;
                                                    if let Some(event_trades) = app.event_trades.get_mut(&event_slug_clone) {
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
