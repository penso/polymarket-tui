use {
    chrono::{DateTime, Utc},
    polymarket_api::{GammaClient, gamma::Event, rtds::RTDSMessage},
    ratatui::{
        Frame, Terminal,
        backend::CrosstermBackend,
        layout::{Alignment, Constraint, Direction, Layout},
        prelude::Stylize,
        style::{Color, Modifier, Style},
        text::Line,
        widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
    },
    std::{io, sync::Arc},
    tokio::sync::Mutex as TokioMutex,
};

/// Macro for conditional debug logging based on tracing feature
#[cfg(feature = "tracing")]
macro_rules! log_debug {
    ($($arg:tt)*) => { tracing::debug!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_debug {
    ($($arg:tt)*) => {};
}

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

pub struct AppState {
    pub trades: Vec<Trade>,
    pub event_slug: String,
    pub should_quit: bool,
    pub event: Option<Event>,
    pub is_loading: bool,
    pub last_refresh: Option<DateTime<Utc>>,
}

impl AppState {
    pub fn new_with_event(event_slug: String, event: Option<Event>) -> Self {
        Self {
            trades: Vec::new(),
            event_slug,
            should_quit: false,
            event,
            is_loading: false,
            last_refresh: None,
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
        // Keep only the last 1000 trades to avoid memory issues
        if self.trades.len() > 1000 {
            self.trades.truncate(1000);
        }
    }
}

pub fn render(f: &mut Frame, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(5), // Event Details
            Constraint::Length(8), // Markets
            Constraint::Min(0),    // Trades Table
            Constraint::Length(3), // Footer
        ])
        .split(f.size());

    // Header
    let loading_indicator = if app.is_loading {
        " [Loading...]"
    } else {
        ""
    };
    let header = Paragraph::new(vec![
        Line::from("💸 Polymarket Trade Monitor".fg(Color::Yellow).bold()),
        Line::from(format!(
            "Event: {} | Trades: {}{}",
            app.event_slug,
            app.trades.len(),
            loading_indicator
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title("Status"))
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: true });
    f.render_widget(header, chunks[0]);

    // Event Details
    render_event_details(f, app, chunks[1]);

    // Markets
    render_markets(f, app, chunks[2]);

    // Trades Table
    if app.trades.is_empty() {
        let empty = Paragraph::new("Waiting for trades...")
            .block(Block::default().borders(Borders::ALL).title("Trades"))
            .alignment(Alignment::Center);
        f.render_widget(empty, chunks[3]);
    } else {
        let rows: Vec<Row> = app
            .trades
            .iter()
            .take(100) // Show last 100 trades
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

                // Pre-compute truncated strings to avoid temporary value issues
                let title_trunc = truncate(&trade.title, 40);
                let user_trunc = truncate(&trade.user, 20);
                let pseudonym_trunc = truncate(&trade.pseudonym, 20);

                Row::new(vec![
                    Cell::from(time).style(Style::default().fg(Color::Gray)),
                    Cell::from(trade.side.clone()).style(side_style),
                    Cell::from(trade.outcome.clone()).style(outcome_style),
                    Cell::from(format!("${:.4}", trade.price)),
                    Cell::from(format!("{:.2}", trade.shares)),
                    Cell::from(format!("${:.2}", trade.total_value)),
                    Cell::from(title_trunc),
                    Cell::from(user_trunc),
                    Cell::from(pseudonym_trunc).style(Style::default().fg(Color::Gray)),
                ])
            })
            .collect();

        let table = Table::new(rows, [
            Constraint::Length(10),     // Time
            Constraint::Length(8),      // Side
            Constraint::Length(5),      // Outcome
            Constraint::Length(10),     // Price
            Constraint::Length(10),     // Shares
            Constraint::Length(10),     // Value
            Constraint::Percentage(25), // Title
            Constraint::Length(20),     // User
            Constraint::Length(20),     // Pseudonym
        ])
        .header(
            Row::new(vec![
                "Time",
                "Side",
                "Out",
                "Price",
                "Shares",
                "Value",
                "Market",
                "User",
                "Pseudonym",
            ])
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Recent Trades"),
        )
        .column_spacing(1);

        f.render_widget(table, chunks[3]);
    }

    // Footer
    let footer = Paragraph::new("Press 'r' to refresh market data | 'q' to quit")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[4]);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn render_event_details(f: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let content = if let Some(event) = &app.event {
        let status = if event.closed {
            "Closed".fg(Color::Red)
        } else if event.active {
            "Active".fg(Color::Green)
        } else {
            "Inactive".fg(Color::Yellow)
        };

        let end_date = event.end_date.as_deref().unwrap_or("N/A");

        let refresh_info = app
            .last_refresh
            .map(|dt| format!(" | Last refresh: {}", dt.format("%H:%M:%S")))
            .unwrap_or_default();

        vec![
            Line::from(vec![
                "Title: ".fg(Color::Gray),
                event.title.clone().fg(Color::White).bold(),
            ]),
            Line::from(vec![
                "Status: ".fg(Color::Gray),
                status,
                " | End: ".fg(Color::Gray),
                end_date.fg(Color::Cyan),
                refresh_info.fg(Color::DarkGray),
            ]),
            Line::from(vec![
                "Markets: ".fg(Color::Gray),
                format!("{}", event.markets.len()).fg(Color::Cyan),
            ]),
        ]
    } else if app.is_loading {
        vec![Line::from("Loading event data...".fg(Color::Yellow))]
    } else {
        vec![Line::from(
            "No event data. Press 'r' to refresh.".fg(Color::Gray),
        )]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Event Details");
    let paragraph = Paragraph::new(content).block(block);
    f.render_widget(paragraph, area);
}

fn render_markets(f: &mut Frame, app: &AppState, area: ratatui::layout::Rect) {
    let content = if let Some(event) = &app.event {
        if event.markets.is_empty() {
            vec![Line::from("No markets available".fg(Color::Gray))]
        } else {
            event
                .markets
                .iter()
                .take(area.height.saturating_sub(2) as usize) // Account for borders
                .map(|market| {
                    // Build outcome prices string - use outcome_prices from Gamma API
                    // (this is what the website displays and is kept up-to-date)
                    let outcomes_str: String = market
                        .outcomes
                        .iter()
                        .enumerate()
                        .map(|(idx, outcome)| {
                            // Use outcome_prices from the fresh Gamma API data
                            let price = market
                                .outcome_prices
                                .get(idx)
                                .and_then(|p| p.parse::<f64>().ok());

                            match price {
                                Some(p) => format!("{}: ${:.2} ({:.0}%)", outcome, p, p * 100.0),
                                None => format!("{}: N/A", outcome),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" | ");

                    // Volume info
                    let volume_24h = market
                        .volume_24hr
                        .map(|v| format!("24h: ${:.0}", v))
                        .unwrap_or_default();

                    let question = truncate(&market.question, 50);
                    Line::from(vec![
                        question.fg(Color::White),
                        " ".into(),
                        outcomes_str.fg(Color::Cyan),
                        " ".into(),
                        volume_24h.fg(Color::Green),
                    ])
                })
                .collect()
        }
    } else if app.is_loading {
        vec![Line::from("Loading markets...".fg(Color::Yellow))]
    } else {
        vec![Line::from(
            "No market data. Press 'r' to refresh.".fg(Color::Gray),
        )]
    };

    let market_count = app.event.as_ref().map(|e| e.markets.len()).unwrap_or(0);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!("Markets ({})", market_count));
    let paragraph = Paragraph::new(content).block(block);
    f.render_widget(paragraph, area);
}

pub async fn refresh_market_data(app_state: Arc<TokioMutex<AppState>>) {
    let event_slug = {
        let mut app = app_state.lock().await;
        app.is_loading = true;
        app.event_slug.clone()
    };

    log_debug!("🔄 Refreshing market data for event: {}", event_slug);

    // Fetch fresh event data from Gamma API (includes current outcome_prices)
    log_debug!("📡 Fetching event from Gamma API: {}", event_slug);
    let gamma_client = GammaClient::new();
    let event_result = gamma_client.get_event_by_slug(&event_slug).await;

    match event_result {
        Ok(Some(event)) => {
            let _market_count = event.markets.len();
            log_debug!("✓ Gamma API returned event with {} markets", _market_count);
            let mut app = app_state.lock().await;
            app.event = Some(event);
            app.is_loading = false;
            app.last_refresh = Some(Utc::now());
            log_debug!("✓ Market data refreshed successfully");
        },
        Ok(None) => {
            log_debug!("⚠ Event not found: {}", event_slug);
            let mut app = app_state.lock().await;
            app.is_loading = false;
        },
        Err(_e) => {
            log_debug!("✗ Failed to fetch event from Gamma API: {}", _e);
            let mut app = app_state.lock().await;
            app.is_loading = false;
        },
    }
}

pub async fn run_tui(
    mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
    app_state: Arc<TokioMutex<AppState>>,
) -> anyhow::Result<()> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    loop {
        // Use lock().await to properly handle async context
        let app = app_state.lock().await;
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .map_err(|e| anyhow::anyhow!("Terminal draw error: {}", e))?;
        drop(app); // Release lock before polling events

        if crossterm::event::poll(std::time::Duration::from_millis(100))
            .map_err(|e| anyhow::anyhow!("Event poll error: {}", e))?
            && let Event::Key(key) =
                event::read().map_err(|e| anyhow::anyhow!("Event read error: {}", e))?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    let mut app = app_state.lock().await;
                    app.should_quit = true;
                    break;
                },
                KeyCode::Char('r') => {
                    // Check if not already loading
                    let is_loading = {
                        let app = app_state.lock().await;
                        app.is_loading
                    };
                    if !is_loading {
                        let app_state_clone = Arc::clone(&app_state);
                        tokio::spawn(async move {
                            refresh_market_data(app_state_clone).await;
                        });
                    }
                },
                _ => {},
            }
        }

        {
            let app = app_state.lock().await;
            if app.should_quit {
                break;
            }
        }
    }

    Ok(())
}
