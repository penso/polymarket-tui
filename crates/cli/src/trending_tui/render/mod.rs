//! Render functions for the trending TUI

mod event_details;
mod events_list;
mod favorites;
mod header;
mod logs;
mod markets;
mod orderbook;
mod popups;
mod trades;
pub mod utils;
mod yield_tab;

use {
    event_details::render_event_details,
    events_list::render_events_list,
    favorites::render_favorites_tab,
    header::render_header,
    logs::render_logs,
    markets::render_markets,
    orderbook::render_orderbook,
    popups::render_popup,
    trades::{render_trades_panel, render_trades_table},
    yield_tab::render_yield_tab,
};
pub use {orderbook::check_orderbook_title_click, utils::truncate};

use {
    super::state::{MainTab, SearchMode, TrendingAppState},
    chrono::{DateTime, Utc},
    polymarket_api::gamma::Event,
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Paragraph},
    },
};

/// Unified tab enum for click detection (combines MainTab and EventFilter)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickedTab {
    Trending, // Displayed as "Events"
    Favorites,
    Breaking,
    Yield,
}

/// Calculate the required height for the orderbook panel based on data
/// Uses actual data size when available, preserves last height when loading
pub(super) fn calculate_orderbook_height(
    app: &TrendingAppState,
    event: Option<&polymarket_api::gamma::Event>,
) -> u16 {
    const MAX_PER_SIDE: usize = 6;
    // Min height for message display = borders(2) + title(1) + message(1) = 4
    const MESSAGE_HEIGHT: u16 = 4;

    // Check if the selected market is closed
    let market_is_closed = event.is_some_and(|e| {
        let mut sorted_markets: Vec<_> = e.markets.iter().collect();
        sorted_markets.sort_by_key(|m| m.closed);
        let idx = app
            .orderbook_state
            .selected_market_index
            .min(sorted_markets.len().saturating_sub(1));
        sorted_markets.get(idx).is_some_and(|m| m.closed)
    });

    if market_is_closed {
        // Closed market - use fixed small height
        MESSAGE_HEIGHT
    } else if app.orderbook_state.is_loading {
        // Keep the same height during loading to prevent layout jumps
        app.orderbook_state.last_height.max(MESSAGE_HEIGHT)
    } else if let Some(orderbook) = &app.orderbook_state.orderbook {
        let asks_count = orderbook.asks.len().min(MAX_PER_SIDE);
        let bids_count = orderbook.bids.len().min(MAX_PER_SIDE);
        // Height = borders(2) + header(1) + asks + spread(1) + bids
        let height = 2 + 1 + asks_count + 1 + bids_count;
        (height as u16).max(MESSAGE_HEIGHT)
    } else {
        // No data yet, use last height or message height
        app.orderbook_state.last_height.max(MESSAGE_HEIGHT)
    }
}

/// Render a search/filter input field with proper styling
/// Returns the cursor position if the field should show a cursor
pub(super) fn render_search_input(
    f: &mut Frame,
    area: Rect,
    query: &str,
    title: &str,
    placeholder: &str,
    is_loading: bool,
    border_color: Color,
) {
    use ratatui::layout::Position;

    // Calculate inner area for the input text
    let inner_x = area.x + 1;
    let inner_y = area.y + 1;
    let inner_width = area.width.saturating_sub(2);

    // Render the block/border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(Style::default().fg(border_color));
    f.render_widget(block, area);

    // Input field area with background
    let input_area = Rect {
        x: inner_x,
        y: inner_y,
        width: inner_width,
        height: 1,
    };

    // Determine display text
    let (display_text, text_style) = if query.is_empty() {
        // Show placeholder with dark background
        (
            placeholder.to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else if is_loading {
        // Show query with loading indicator
        (
            format!("{} (searching...)", query),
            Style::default().fg(Color::Cyan).bold(),
        )
    } else {
        // Show query
        (query.to_string(), Style::default().fg(Color::White).bold())
    };

    // Pad to fill the field width (creates visible input area with background)
    let padded_text = format!("{:<width$}", display_text, width = inner_width as usize);

    // Use background color to make input field visible
    let input_para = Paragraph::new(padded_text).style(text_style.bg(Color::Rgb(40, 40, 40)));
    f.render_widget(input_para, input_area);

    // Set cursor position at end of query text
    if !query.is_empty() || is_loading {
        // Don't show cursor when loading
        if !is_loading {
            let cursor_x = inner_x + query.len().min(inner_width as usize - 1) as u16;
            f.set_cursor_position(Position::new(cursor_x, inner_y));
        }
    } else {
        // Show cursor at start for empty field
        f.set_cursor_position(Position::new(inner_x, inner_y));
    }
}

/// Check if the login button was clicked (top right)
/// Returns true if click is on the login button area
pub fn is_login_button_clicked(x: u16, y: u16, size: Rect, app: &TrendingAppState) -> bool {
    // Login button is on the first line (y = 0) and at the right edge
    if y != 0 {
        return false;
    }

    // Calculate button width dynamically based on auth state
    let button_width = if app.auth_state.is_authenticated {
        let name = app.auth_state.display_name();
        (name.len() + 4) as u16 // "[ " + name + " ]"
    } else {
        10 // "[ Login ]"
    };

    let login_button_start = size.width.saturating_sub(button_width);
    x >= login_button_start
}

/// Tabs are rendered on the first line (y = 0)
/// Returns which tab was clicked: Trending [1], Breaking [2], New [3], Yield [4]
pub fn get_clicked_tab(x: u16, y: u16, size: Rect, app: &TrendingAppState) -> Option<ClickedTab> {
    // Tabs are on the first line (y = 0)
    if y != 0 {
        return None;
    }

    // Don't match tabs if clicking on login button area (right side)
    // Calculate button width dynamically based on auth state
    let button_width = if app.auth_state.is_authenticated {
        let name = app.auth_state.display_name();
        (name.len() + 4) as u16 // "[ " + name + " ]"
    } else {
        10 // "[ Login ]"
    };
    let login_button_start = size.width.saturating_sub(button_width);
    if x >= login_button_start {
        return None;
    }

    // Actual rendered output (Tabs widget adds leading space and " " divider):
    // " Events [1] Favorites [2] Breaking [3] Yield [4]"
    // 0         1         2         3         4         5
    // 012345678901234567890123456789012345678901234567890
    //  Events [1] Favorites [2] Breaking [3] Yield [4]
    // Positions: 1-10 = Events, 12-25 = Favorites, 27-38 = Breaking, 40-49 = Yield
    if x <= 10 {
        return Some(ClickedTab::Trending);
    } else if (12..26).contains(&x) {
        return Some(ClickedTab::Favorites);
    } else if (27..39).contains(&x) {
        return Some(ClickedTab::Breaking);
    } else if (40..50).contains(&x) {
        return Some(ClickedTab::Yield);
    }
    None
}

/// Shared function to build event info lines for display
/// Used by both Events tab and Yield tab to show consistent event details
fn build_event_info_lines(
    event: &Event,
    is_watching: bool,
    trade_count_display: &str,
    trade_label: &str,
    area_width: u16,
) -> Vec<Line<'static>> {
    // Calculate total volume from all markets
    let total_volume: f64 = event
        .markets
        .iter()
        .map(|m| m.volume_24hr.or(m.volume_total).unwrap_or(0.0))
        .sum();

    // Format end date with relative time
    let end_date_str = event
        .end_date
        .as_ref()
        .and_then(|date_str| {
            DateTime::parse_from_rfc3339(date_str)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
        .map(|dt| {
            let now = Utc::now();
            let duration = dt.signed_duration_since(now);
            if duration.num_days() > 0 {
                format!("{} days", duration.num_days())
            } else if duration.num_hours() > 0 {
                format!("{} hours", duration.num_hours())
            } else if duration.num_minutes() > 0 {
                format!("{} min", duration.num_minutes())
            } else if duration.num_seconds() < 0 {
                format!("Expired ({})", dt.format("%Y-%m-%d %H:%M UTC"))
            } else {
                format!("{}", dt.format("%Y-%m-%d %H:%M UTC"))
            }
        })
        .unwrap_or_else(|| "N/A".to_string());

    // Format volume
    let volume_str = if total_volume >= 1_000_000.0 {
        format!("${:.1}M", total_volume / 1_000_000.0)
    } else if total_volume >= 1_000.0 {
        format!("${:.1}K", total_volume / 1_000.0)
    } else {
        format!("${:.0}", total_volume)
    };

    let event_url = format!("https://polymarket.com/event/{}", event.slug);

    // Build lines
    let mut lines = vec![
        // Slug
        Line::from(vec![
            Span::styled("Slug: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(truncate(&event.slug, 60), Style::default().fg(Color::Blue)),
        ]),
        // URL
        Line::from(vec![
            Span::styled("URL: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(event_url, Style::default().fg(Color::Cyan)),
        ]),
        // Status: Active/Inactive | Open/Closed | Watching
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(
                if event.active {
                    "Active"
                } else {
                    "Inactive"
                },
                Style::default().fg(if event.active {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
            Span::styled(" | ", Style::default().fg(Color::Gray)),
            Span::styled(
                if event.closed {
                    "Closed"
                } else {
                    "Open"
                },
                Style::default().fg(if event.closed {
                    Color::Red
                } else {
                    Color::Green
                }),
            ),
            Span::styled(" | ", Style::default().fg(Color::Gray)),
            Span::styled(
                if is_watching {
                    "Watching"
                } else {
                    "Not Watching"
                },
                Style::default().fg(if is_watching {
                    Color::Red
                } else {
                    Color::Gray
                }),
            ),
        ]),
        // Estimated End
        Line::from(vec![
            Span::styled("Estimated End: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(end_date_str, Style::default().fg(Color::Magenta)),
        ]),
        // Total Volume | Trades
        Line::from(vec![
            Span::styled("Total Volume: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(
                volume_str,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}: ", trade_label),
                Style::default().fg(Color::Yellow).bold(),
            ),
            Span::styled(
                trade_count_display.to_string(),
                Style::default().fg(if trade_label == "Your Trades" {
                    Color::Green
                } else if trade_count_display == "..." {
                    Color::Yellow
                } else if is_watching {
                    Color::Cyan
                } else {
                    Color::Gray
                }),
            ),
        ]),
    ];

    // Add tags if available
    if !event.tags.is_empty() {
        let tag_labels: Vec<String> = event
            .tags
            .iter()
            .map(|tag| truncate(&tag.label, 20))
            .collect();
        let tags_text = tag_labels.join(", ");

        // Calculate available width for tags
        let available_width = (area_width as usize).saturating_sub(8);
        let tags_char_count = tags_text.chars().count();

        if tags_char_count <= available_width {
            lines.push(Line::from(vec![
                Span::styled("Tags: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(tags_text, Style::default().fg(Color::Cyan)),
            ]));
        } else {
            // Truncate tags if too long
            lines.push(Line::from(vec![
                Span::styled("Tags: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(
                    truncate(&tags_text, available_width),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
        }
    }

    lines
}

pub fn render(f: &mut Frame, app: &mut TrendingAppState) {
    // Header height: 2 lines for normal mode (tabs + separator), 5 for search mode
    let header_height = if app.is_in_filter_mode() {
        5
    } else {
        2
    };
    // No overlap - all panels have full borders with rounded corners
    // Conditionally include logs area based on show_logs
    let constraints: Vec<Constraint> = if app.show_logs {
        vec![
            Constraint::Length(header_height), // Header (with search if active)
            Constraint::Min(0),                // Main content
            Constraint::Length(8),             // Logs area
            Constraint::Length(3),             // Footer
        ]
    } else {
        vec![
            Constraint::Length(header_height), // Header (with search if active)
            Constraint::Min(0),                // Main content
            Constraint::Length(3),             // Footer
        ]
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(f.area());

    // Render header with main tabs
    render_header(f, app, chunks[0]);

    // Main content depends on active main tab
    match app.main_tab {
        MainTab::Trending => {
            // Main content - split into events list and trades view
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(40), // Events list
                    Constraint::Fill(1),        // Right side takes remaining space
                ])
                .split(chunks[1]);

            render_events_list(f, app, main_chunks[0]);
            render_trades(f, app, main_chunks[1]);
        },
        MainTab::Favorites => {
            render_favorites_tab(f, app, chunks[1]);
        },
        MainTab::Yield => {
            render_yield_tab(f, app, chunks[1]);
        },
    }

    // Logs area (only if shown)
    // Footer index depends on whether logs are shown
    let footer_idx = if app.show_logs {
        render_logs(f, app, chunks[2]);
        3
    } else {
        2
    };

    // Footer - show focused panel info with context-sensitive help
    let panel_name = app.navigation.focused_panel.name();
    let panel_help = if app.main_tab == MainTab::Yield {
        "/: Search | f: Filter | s: Sort | r: Refresh | o: Open"
    } else {
        app.navigation.focused_panel.help_text()
    };
    let footer_text = if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
        "Type to search | Esc: Cancel".to_string()
    } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
        "Type to filter | Esc: Cancel".to_string()
    } else if app.search.mode == SearchMode::ApiSearch {
        "Type to search | Esc: Cancel".to_string()
    } else if app.search.mode == SearchMode::LocalFilter {
        "Type to filter | Esc: Cancel".to_string()
    } else {
        format!(
            "{} | b: Bookmark | p: Profile | l: Logs | q: Quit | [{}]",
            panel_help, panel_name
        )
    };
    let footer = Paragraph::new(footer_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[footer_idx]);

    // Render popup if active (on top of everything)
    if let Some(ref popup) = app.popup {
        render_popup(f, app, popup);
    }
}

fn render_trades(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    if let Some(event) = app.selected_event() {
        let event_slug = &event.slug;
        let trades = app.get_trades(event_slug);
        let is_watching = app.is_watching(event_slug);

        // Use a fixed minimum height for event details panel
        // Content will scroll if it exceeds this height
        let min_event_details_height = 8; // Minimum height (6 base lines + 2 for borders)

        // Calculate dynamic orderbook height based on data
        let orderbook_height = calculate_orderbook_height(app, Some(event));

        // Split area into event details, markets, orderbook, and trades
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(min_event_details_height as u16), // Event details (minimum height, scrollable)
                Constraint::Length(7),                // Markets panel (5 lines + 2 for borders)
                Constraint::Length(orderbook_height), // Order Book panel (dynamic)
                Constraint::Min(0),                   // Trades table
            ])
            .split(area);

        // Render event details
        render_event_details(f, app, event, is_watching, trades.len(), chunks[0]);

        // Render markets panel
        render_markets(f, app, event, chunks[1]);

        // Render order book panel
        render_orderbook(f, app, event, chunks[2]);

        // Render trades table
        render_trades_table(f, app, trades, Some(event), is_watching, chunks[3]);
    } else {
        let paragraph = Paragraph::new("No event selected")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_type(BorderType::Rounded)
                    .title("Event Details & Trades"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(paragraph, area);
    }
}
