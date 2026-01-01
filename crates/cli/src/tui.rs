use chrono::DateTime;
use polymarket_bot::rtds::RTDSMessage;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    prelude::Stylize,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
    Frame, Terminal,
};
use std::io;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

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
}

impl AppState {
    pub fn new(event_slug: String) -> Self {
        Self {
            trades: Vec::new(),
            event_slug,
            should_quit: false,
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
            Constraint::Min(0),    // Table
            Constraint::Length(3), // Footer
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new(vec![
        Line::from("💸 Polymarket Trade Monitor".fg(Color::Yellow).bold()),
        Line::from(format!(
            "Event: {} | Trades: {}",
            app.event_slug,
            app.trades.len()
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title("Status"))
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: true });
    f.render_widget(header, chunks[0]);

    // Table
    if app.trades.is_empty() {
        let empty = Paragraph::new("Waiting for trades...")
            .block(Block::default().borders(Borders::ALL).title("Trades"))
            .alignment(Alignment::Center);
        f.render_widget(empty, chunks[1]);
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
                    Cell::from(if trade.side == "BUY" {
                        "🟢 BUY"
                    } else {
                        "🔴 SELL"
                    })
                    .style(side_style),
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

        let table = Table::new(
            rows,
            [
                Constraint::Length(10),     // Time
                Constraint::Length(8),      // Side
                Constraint::Length(5),      // Outcome
                Constraint::Length(10),     // Price
                Constraint::Length(10),     // Shares
                Constraint::Length(10),     // Value
                Constraint::Percentage(25), // Title
                Constraint::Length(20),     // User
                Constraint::Length(20),     // Pseudonym
            ],
        )
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

        f.render_widget(table, chunks[1]);
    }

    // Footer
    let footer = Paragraph::new("Press 'q' or Ctrl+C to quit")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[2]);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
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
        {
            if let Event::Key(key) =
                event::read().map_err(|e| anyhow::anyhow!("Event read error: {}", e))?
            {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            let mut app = app_state.lock().await;
                            app.should_quit = true;
                            break;
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

    Ok(())
}
