//! Render functions for the trending TUI

use {
    super::state::{
        EventFilter, FocusedPanel, LoginField, MainTab, PopupType, SearchMode, TrendingAppState,
    },
    chrono::{DateTime, Utc},
    polymarket_api::gamma::Event,
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{
            Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row,
            Scrollbar, ScrollbarOrientation, ScrollbarState, Table, Tabs, Wrap,
        },
    },
    unicode_width::UnicodeWidthStr,
};

/// Unified tab enum for click detection (combines MainTab and EventFilter)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickedTab {
    Trending, // Displayed as "Events"
    Favorites,
    Breaking,
    Yield,
}

/// Render a search/filter input field with proper styling
/// Returns the cursor position if the field should show a cursor
fn render_search_input(
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

/// Yield opportunity threshold (95% probability = 5% potential return)
const YIELD_MIN_PROB: f64 = 0.95;

/// Check if a market has a yield opportunity (any outcome with price >= 95% and < 100%)
fn market_has_yield(market: &polymarket_api::gamma::Market) -> bool {
    // Skip closed/resolved markets - no yield opportunity
    if market.closed {
        return false;
    }

    market.outcome_prices.iter().any(|price_str| {
        price_str
            .parse::<f64>()
            .ok()
            .is_some_and(|price| (YIELD_MIN_PROB..1.0).contains(&price))
    })
}

/// Check if an event has any yield opportunities (any market with high probability outcome)
fn event_has_yield(event: &polymarket_api::gamma::Event) -> bool {
    event.markets.iter().any(market_has_yield)
}

/// Format a price (0.0-1.0) as cents like the Polymarket website
/// Examples: 0.01 -> "1¢", 0.11 -> "11¢", 0.89 -> "89¢", 0.004 -> "<1¢", 0.9995 -> "99.95¢"
fn format_price_cents(price: f64) -> String {
    let cents = price * 100.0;
    if cents < 1.0 {
        "<1¢".to_string()
    } else if cents < 10.0 {
        format!("{:.1}¢", cents)
    } else if (99.0..100.0).contains(&cents) {
        // Show more precision for high prices (99-100%) to distinguish yields
        format!("{:.2}¢", cents)
    } else {
        format!("{:.0}¢", cents)
    }
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
                if event.active { "Active" } else { "Inactive" },
                Style::default().fg(if event.active {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
            Span::styled(" | ", Style::default().fg(Color::Gray)),
            Span::styled(
                if event.closed { "Closed" } else { "Open" },
                Style::default().fg(if event.closed {
                    Color::Red
                } else {
                    Color::Green
                }),
            ),
            Span::styled(" | ", Style::default().fg(Color::Gray)),
            Span::styled(
                if is_watching { "Watching" } else { "Not Watching" },
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
                Span::styled(truncate(&tags_text, available_width), Style::default().fg(Color::Cyan)),
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

fn render_header(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    // Calculate unified tab index: 0=Events, 1=Favorites, 2=Breaking, 3=Yield
    let tab_index = match app.main_tab {
        MainTab::Trending => match app.event_filter {
            EventFilter::Trending => 0,
            EventFilter::Breaking => 2,
        },
        MainTab::Favorites => 1,
        MainTab::Yield => 3,
    };

    if app.is_in_filter_mode() {
        // Split header into tabs, separator, and search input
        let header_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tabs line
                Constraint::Length(1), // Horizontal separator
                Constraint::Length(3), // Search input
            ])
            .split(area);

        // Render unified tabs
        let tab_titles: Vec<Line> = vec![
            Line::from("Events [1]"),
            Line::from("Favorites [2]"),
            Line::from("Breaking [3]"),
            Line::from("Yield [4]"),
        ];
        let tabs = Tabs::new(tab_titles)
            .select(tab_index)
            .style(Style::default().fg(Color::DarkGray))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
            .divider(" ");
        f.render_widget(tabs, header_chunks[0]);

        // Horizontal separator line (gitui-style) - full width line of ─ characters
        let line_width = header_chunks[1].width as usize;
        let separator_line = "─".repeat(line_width);
        let separator = Paragraph::new(separator_line).style(Style::default().fg(Color::DarkGray));
        f.render_widget(separator, header_chunks[1]);

        // Search input field with proper styling
        let placeholder = match app.search.mode {
            SearchMode::ApiSearch => "Type to search via API...",
            SearchMode::LocalFilter => "Type to filter current list...",
            SearchMode::None => "Type to search...",
        };
        let title = if app.search.is_searching {
            "Search (loading...)"
        } else {
            "Search (Esc to close)"
        };
        render_search_input(
            f,
            header_chunks[2],
            &app.search.query,
            title,
            placeholder,
            app.search.is_searching,
            Color::Yellow,
        );
    } else {
        // Normal mode: Split header into tabs and horizontal line
        let header_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tabs line
                Constraint::Length(1), // Horizontal separator line
            ])
            .split(area);

        // Build right side: portfolio info + profile button
        let mut right_spans: Vec<Span> = Vec::new();

        // Add portfolio info if authenticated and available
        if app.auth_state.is_authenticated {
            // Total value (cash + portfolio)
            if app.auth_state.balance.is_some() || app.auth_state.portfolio_value.is_some() {
                let total = app.auth_state.balance.unwrap_or(0.0)
                    + app.auth_state.portfolio_value.unwrap_or(0.0);
                right_spans.push(Span::styled(
                    format!("${:.0}", total),
                    Style::default().fg(Color::Green),
                ));
                right_spans.push(Span::raw(" "));
            }

            // P&L
            if app.auth_state.unrealized_pnl.is_some() || app.auth_state.realized_pnl.is_some() {
                let total_pnl = app.auth_state.unrealized_pnl.unwrap_or(0.0)
                    + app.auth_state.realized_pnl.unwrap_or(0.0);
                let (pnl_str, pnl_color) = if total_pnl.abs() < 0.005 {
                    ("$0".to_string(), Color::DarkGray)
                } else if total_pnl > 0.0 {
                    (format!("+${:.0}", total_pnl), Color::Green)
                } else {
                    (format!("-${:.0}", total_pnl.abs()), Color::Red)
                };
                right_spans.push(Span::styled(pnl_str, Style::default().fg(pnl_color)));
                right_spans.push(Span::raw(" "));
            }

            // Profile button
            let name = app.auth_state.display_name();
            right_spans.push(Span::styled(
                format!("[ {} ]", name),
                Style::default().fg(Color::Green),
            ));
        } else {
            right_spans.push(Span::styled("[ Login ]", Style::default().fg(Color::Cyan)));
        }

        let right_line = Line::from(right_spans);
        let right_width = right_line.width() as u16;

        // Split tabs line: tabs on left, portfolio + button on right
        let tabs_line_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),              // Tabs (fill remaining space)
                Constraint::Length(right_width), // Portfolio info + button
            ])
            .split(header_chunks[0]);

        // Render unified tabs in gitui-style (underline for selected, keyboard shortcuts)
        let tab_titles: Vec<Line> = vec![
            Line::from("Events [1]"),
            Line::from("Favorites [2]"),
            Line::from("Breaking [3]"),
            Line::from("Yield [4]"),
        ];
        let tabs = Tabs::new(tab_titles)
            .select(tab_index)
            .style(Style::default().fg(Color::DarkGray))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
            .divider(" ");
        f.render_widget(tabs, tabs_line_chunks[0]);

        // Render portfolio info + login/user button on the right
        let right_paragraph = Paragraph::new(right_line).alignment(Alignment::Right);
        f.render_widget(right_paragraph, tabs_line_chunks[1]);

        // Horizontal separator line (gitui-style) - full width line of ─ characters
        let line_width = header_chunks[1].width as usize;
        let separator_line = "─".repeat(line_width);
        let separator = Paragraph::new(separator_line).style(Style::default().fg(Color::DarkGray));
        f.render_widget(separator, header_chunks[1]);
    }
}

/// Render the favorites tab
fn render_favorites_tab(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let favorites_state = &app.favorites_state;

    // Check authentication first
    if !app.auth_state.is_authenticated {
        let message = Paragraph::new("Please login to view your favorites.\n\nPress Tab to go to Login button, then Enter to open login dialog.")
            .block(
                Block::default()
                    .title(" Favorites ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(message, area);
        return;
    }

    // Show loading state - use same layout but with loading indicator
    if favorites_state.is_loading {
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // Events list
                Constraint::Fill(1),        // Right side
            ])
            .split(area);

        // Events panel with "Loading..." title
        let loading_list = Paragraph::new("Loading favorites...")
            .block(
                Block::default()
                    .title(" Events (Loading...) ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(loading_list, main_chunks[0]);

        // Empty right panel
        let empty_details = Paragraph::new("").block(
            Block::default()
                .title(" Event Details ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
        f.render_widget(empty_details, main_chunks[1]);
        return;
    }

    // Show error/info state
    if let Some(ref error) = favorites_state.error_message {
        // Check if this is a "missing session cookie" info message vs actual error
        let is_session_cookie_missing =
            error.contains("session_cookie") || error.contains("Session cookie");

        if is_session_cookie_missing {
            // Get the actual config path
            let config_path = crate::auth::AuthConfig::config_path();
            let config_path_str = config_path.display().to_string();

            // Show helpful setup instructions, not an error
            let lines = vec![
                Line::from(Span::styled(
                    "Session Cookie Required",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("Favorites require browser authentication."),
                Line::from(""),
                Line::from(Span::styled(
                    "To set up:",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from("1. Log in to polymarket.com in your browser"),
                Line::from("2. Open Developer Tools (F12)"),
                Line::from("3. Go to Application > Cookies > polymarket.com"),
                Line::from("4. Copy these cookie values and add to config:"),
                Line::from(""),
                Line::from(Span::styled(
                    format!("   {}", config_path_str),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "   \"session_cookie\": \"<polymarketsession>\",",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(Span::styled(
                    "   \"session_nonce\": \"<polymarketnonce>\",",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(Span::styled(
                    "   \"session_auth_type\": \"magic\"",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press 'e' to open config in system editor",
                    Style::default().fg(Color::Green),
                )),
            ];

            let info_msg = Paragraph::new(lines)
                .block(
                    Block::default()
                        .title(" Favorites - Setup Required ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Yellow)),
                )
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: true });
            f.render_widget(info_msg, area);
        } else {
            // Show actual error
            let error_msg = Paragraph::new(format!("Error: {}", error))
                .block(
                    Block::default()
                        .title(" Favorites ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded),
                )
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(Color::Red));
            f.render_widget(error_msg, area);
        }
        return;
    }

    // Show empty state
    if favorites_state.events.is_empty() {
        let empty = Paragraph::new("No favorites yet.\n\nBrowse events in the Events tab and press 'b' to bookmark them.")
            .block(
                Block::default()
                    .title(" Favorites ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty, area);
        return;
    }

    // Use the same layout as Trending tab - events list + trades view
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // Events list
            Constraint::Fill(1),        // Right side takes remaining space
        ])
        .split(area);

    render_favorites_list(f, app, main_chunks[0]);
    render_trades(f, app, main_chunks[1]);
}

/// Render the favorites events list (separate from main events list)
fn render_favorites_list(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let favorites_state = &app.favorites_state;
    let events = &favorites_state.events;

    let scroll = favorites_state.scroll;
    let selected_index = favorites_state.selected_index;
    let visible_events: Vec<_> = events
        .iter()
        .enumerate()
        .skip(scroll)
        .take(area.height as usize - 2)
        .collect();

    // First pass: calculate max width of market count for alignment
    let max_markets_width = visible_events
        .iter()
        .map(|(_, event)| event.markets.len().to_string().len())
        .max()
        .unwrap_or(1);

    let items: Vec<ListItem> = visible_events
        .into_iter()
        .map(|(idx, event)| {
            let is_selected = idx == selected_index;

            // Check if event is closed/inactive
            let is_closed = event.closed || !event.active;

            // Calculate total volume from markets
            let total_volume: f64 = event
                .markets
                .iter()
                .map(|m| m.volume_24hr.or(m.volume_total).unwrap_or(0.0))
                .sum();

            // Format volume
            let volume_str = if total_volume >= 1_000_000.0 {
                format!("${:.1}M", total_volume / 1_000_000.0)
            } else if total_volume >= 1_000.0 {
                format!("${:.1}K", total_volume / 1_000.0)
            } else {
                format!("${:.0}", total_volume)
            };

            // Format market count with padding
            let markets_str = format!("{:>width$}", event.markets.len(), width = max_markets_width);

            // Build the display line with favorite icon (all favorites are bookmarked)
            let mut spans = vec![Span::styled("⚑ ", Style::default().fg(Color::Magenta))];

            // Check for yield opportunities
            if event_has_yield(event) {
                spans.push(Span::styled("$ ", Style::default().fg(Color::Green)));
            }

            // Title with appropriate styling
            let title_style = if is_closed {
                Style::default().fg(Color::DarkGray)
            } else if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(truncate(&event.title, 40), title_style));

            // Volume
            spans.push(Span::styled(
                format!(" {}", volume_str),
                Style::default().fg(if is_closed {
                    Color::DarkGray
                } else {
                    Color::Green
                }),
            ));

            // Market count
            spans.push(Span::styled(
                format!(" {}", markets_str),
                Style::default().fg(if is_closed {
                    Color::DarkGray
                } else {
                    Color::Cyan
                }),
            ));

            let line = Line::from(spans);
            let mut item = ListItem::new(line);

            if is_selected {
                item = item.style(
                    Style::default()
                        .bg(Color::Rgb(60, 60, 80))
                        .add_modifier(Modifier::BOLD),
                );
            }

            item
        })
        .collect();

    let is_focused = app.navigation.focused_panel == FocusedPanel::EventsList;
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let title = format!(" Favorites ({}) ", events.len());
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title)
                .border_style(block_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(60, 60, 80))
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(list, area);

    // Render scrollbar if needed
    let total_items = events.len();
    let visible_height = area.height.saturating_sub(2) as usize;
    if total_items > visible_height {
        let mut scrollbar_state = ScrollbarState::new(total_items)
            .position(scroll)
            .viewport_content_length(visible_height);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut scrollbar_state,
        );
    }
}

/// Render the yield opportunities tab
fn render_yield_tab(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let yield_state = &app.yield_state;

    // If searching, add a search input area at the top
    if yield_state.is_searching {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search input
                Constraint::Min(0),    // Main content
            ])
            .split(area);

        // Render search input with proper styling
        render_search_input(
            f,
            chunks[0],
            &yield_state.search_query,
            "Search (Esc to close)",
            "Type to search events...",
            yield_state.is_search_loading,
            Color::Yellow,
        );

        // Render main content below search
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(55), // Results list
                Constraint::Fill(1),        // Details panel
            ])
            .split(chunks[1]);

        // Show search results if available, otherwise show normal yield list
        if !yield_state.search_results.is_empty() || !yield_state.last_searched_query.is_empty() {
            render_yield_search_results(f, app, main_chunks[0]);
            render_yield_search_details(f, app, main_chunks[1]);
        } else {
            render_yield_list(f, app, main_chunks[0]);
            render_yield_details(f, app, main_chunks[1]);
        }
    // If filtering, add a filter input area at the top
    } else if yield_state.is_filtering {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Filter input
                Constraint::Min(0),    // Main content
            ])
            .split(area);

        // Render filter input with proper styling
        render_search_input(
            f,
            chunks[0],
            &yield_state.filter_query,
            "Filter (Esc to close)",
            "Type to filter by event/market name...",
            false, // Filter is local, never loading
            Color::Yellow,
        );

        // Render main content below filter
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(55), // Opportunities list
                Constraint::Fill(1),        // Details panel
            ])
            .split(chunks[1]);

        render_yield_list(f, app, main_chunks[0]);
        render_yield_details(f, app, main_chunks[1]);
    } else {
        // Normal mode: Split into list on left and details on right
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(55), // Opportunities list
                Constraint::Fill(1),        // Details panel
            ])
            .split(area);

        render_yield_list(f, app, chunks[0]);
        render_yield_details(f, app, chunks[1]);
    }
}

fn render_yield_list(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let yield_state = &app.yield_state;

    if yield_state.is_loading {
        let loading = Paragraph::new("Loading yield opportunities...")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Yield Opportunities"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(loading, area);
        return;
    }

    if yield_state.opportunities.is_empty() {
        let empty = Paragraph::new("No yield opportunities found.\nPress 'r' to refresh.")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Yield Opportunities"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(empty, area);
        return;
    }

    // Get filtered opportunities
    let filtered = yield_state.filtered_opportunities();

    if filtered.is_empty() {
        let empty = Paragraph::new(format!(
            "No matches for '{}'\nPress Esc to clear filter.",
            yield_state.filter_query
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Yield Opportunities (filtered)"),
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));
        f.render_widget(empty, area);
        return;
    }

    // Calculate visible height (accounting for borders and header row)
    let visible_height = (area.height as usize).saturating_sub(3); // -2 borders, -1 header
    let total_items = filtered.len();
    let scroll = yield_state
        .scroll
        .min(total_items.saturating_sub(visible_height.max(1)));

    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(idx, opp)| {
            // Look up event from cache for title and end date
            let cached_event = app.get_cached_event(&opp.event_slug);
            let event_title = cached_event
                .map(|e| e.title.as_str())
                .unwrap_or(&opp.event_slug);

            // Format return with color based on value
            let return_color = if opp.est_return >= 5.0 {
                Color::Green
            } else if opp.est_return >= 2.0 {
                Color::Yellow
            } else {
                Color::Red
            };

            // Format volume
            let volume_str = if opp.volume >= 1_000_000.0 {
                format!("${:.1}M", opp.volume / 1_000_000.0)
            } else if opp.volume >= 1_000.0 {
                format!("${:.0}K", opp.volume / 1_000.0)
            } else if opp.volume > 0.0 {
                format!("${:.0}", opp.volume)
            } else {
                "-".to_string()
            };

            // Format end date from cached event
            let end_str = cached_event
                .and_then(|e| e.end_date.as_ref())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .map(|d| {
                    let now = Utc::now();
                    let days = (d - now).num_days();
                    if days < 0 {
                        "expired".to_string()
                    } else if days == 0 {
                        "today".to_string()
                    } else if days == 1 {
                        "1d".to_string()
                    } else if days < 30 {
                        format!("{}d", days)
                    } else {
                        format!("{}mo", days / 30)
                    }
                })
                .unwrap_or_else(|| "N/A".to_string());

            let return_str = format!("{:.2}%", opp.est_return);
            let price_str = format_price_cents(opp.price);

            // Check if event is favorited
            let is_favorite = app.favorites_state.is_favorite(&opp.event_slug);

            // Create a cell with favorite icon, event title (dimmed) and market name
            // Let the table handle truncation based on column width
            let mut name_spans = Vec::new();
            if is_favorite {
                name_spans.push(Span::styled("⚑ ", Style::default().fg(Color::Magenta)));
            }
            name_spans.push(Span::styled(
                event_title.to_string(),
                Style::default().fg(Color::DarkGray),
            ));
            name_spans.push(Span::styled(" > ", Style::default().fg(Color::DarkGray)));
            name_spans.push(Span::styled(
                opp.market_name.clone(),
                Style::default().fg(Color::White),
            ));
            let name_cell = Cell::from(Line::from(name_spans));

            // Zebra striping
            let bg_color = if idx % 2 == 0 {
                Color::Reset
            } else {
                Color::Rgb(30, 30, 40)
            };

            Row::new(vec![
                name_cell,
                Cell::from(return_str).style(Style::default().fg(return_color)),
                Cell::from(price_str).style(Style::default().fg(Color::Cyan)),
                Cell::from(volume_str).style(Style::default().fg(Color::Green)),
                Cell::from(end_str).style(Style::default().fg(Color::Magenta)),
            ])
            .style(Style::default().bg(bg_color))
        })
        .collect();

    let is_focused = app.navigation.focused_panel == FocusedPanel::EventsList;
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    // Build title with filter info if active
    let title = if !yield_state.filter_query.is_empty() {
        format!(
            "Yield ({}/{}) - Filter: '{}' - Sort: {}",
            filtered.len(),
            yield_state.opportunities.len(),
            truncate(&yield_state.filter_query, 15),
            yield_state.sort_by.label()
        )
    } else {
        format!(
            "Yield Opportunities ({}) - Sort: {}",
            yield_state.opportunities.len(),
            yield_state.sort_by.label()
        )
    };

    // Build block with optional bottom title for loading/searching status
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(block_style);

    if yield_state.is_search_loading {
        block = block.title_bottom(Line::from(" Searching... ").centered());
    }

    let table = Table::new(
        rows,
        [
            Constraint::Fill(1),   // Market name (takes remaining space)
            Constraint::Length(7), // Return (e.g., "12.34%")
            Constraint::Length(7), // Price (e.g., "95.5¢")
            Constraint::Length(8), // Volume (e.g., "$123.4K")
            Constraint::Length(7), // Expires (e.g., "expired")
        ],
    )
    .header(
        Row::new(vec!["Market", "Return", "Price", "Volume", "Expires"])
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(0),
    )
    .block(block)
    .column_spacing(1)
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(60, 60, 80))
            .add_modifier(Modifier::BOLD),
    );

    // Use TableState for row selection
    let mut table_state = ratatui::widgets::TableState::default();
    table_state.select(Some(yield_state.selected_index.saturating_sub(scroll)));
    f.render_stateful_widget(table, area, &mut table_state);

    // Render scrollbar if needed
    let total_items = yield_state.opportunities.len();
    if total_items > visible_height {
        let mut scrollbar_state = ScrollbarState::new(total_items)
            .position(yield_state.scroll)
            .viewport_content_length(visible_height);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut scrollbar_state,
        );
    }
}

fn render_yield_details(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let yield_state = &app.yield_state;

    if let Some(opp) = yield_state.selected_opportunity() {
        // Look up the event from the global cache
        if let Some(event) = app.get_cached_event(&opp.event_slug) {
            // Calculate dynamic height for event info based on content
            // Base: 5 lines (Slug, URL, Status, End, Volume) + 2 for borders + 1 for title
            // Add 1 if tags present, add extra lines for URL wrapping
            let event_url = format!("https://polymarket.com/event/{}", event.slug);
            let available_width = area.width.saturating_sub(8) as usize; // Account for borders and "URL: " prefix
            let url_lines = (event_url.len() / available_width.max(1)) + 1;
            let has_tags = !event.tags.is_empty();
            let event_panel_height = 8 + url_lines as u16 + if has_tags { 1 } else { 0 };

            // Split into event info and market details
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(event_panel_height), // Dynamic event info height
                    Constraint::Min(0),                     // Market details
                ])
                .split(area);

            // Use shared function to build event info lines
            // Yield tab doesn't track watching status or trades, so use defaults
            let event_lines = build_event_info_lines(event, false, "-", "Trades", chunks[0].width);

            let is_details_focused = app.navigation.focused_panel == FocusedPanel::EventDetails;
            let event_block_style = if is_details_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            // Build title with event name
            let title_max_width = chunks[0].width.saturating_sub(12) as usize;
            let title = format!("Event: {}", truncate(&event.title, title_max_width));

            let event_info = Paragraph::new(event_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(title)
                        .border_style(event_block_style),
                )
                .wrap(Wrap { trim: true });
            f.render_widget(event_info, chunks[0]);

            // Market details panel
            let return_color = if opp.est_return >= 5.0 {
                Color::Green
            } else if opp.est_return >= 2.0 {
                Color::Yellow
            } else {
                Color::Red
            };

            let market_volume_str = if opp.volume >= 1_000_000.0 {
                format!("${:.1}M", opp.volume / 1_000_000.0)
            } else if opp.volume >= 1_000.0 {
                format!("${:.1}K", opp.volume / 1_000.0)
            } else {
                format!("${:.0}", opp.volume)
            };

            let market_lines = vec![
                Line::from(vec![
                    Span::styled("Market: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(opp.market_name.clone(), Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(
                        opp.market_status,
                        Style::default().fg(if opp.market_status == "open" {
                            Color::Green
                        } else {
                            Color::Red
                        }),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Outcome: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(
                        opp.outcome.clone(),
                        Style::default().fg(if opp.outcome == "Yes" {
                            Color::Green
                        } else {
                            Color::Red
                        }),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Price: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(
                        format!(
                            "{} ({:.2}%)",
                            format_price_cents(opp.price),
                            opp.price * 100.0
                        ),
                        Style::default().fg(Color::Cyan),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Est. Return: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(
                        format!("{:.2}%", opp.est_return),
                        Style::default()
                            .fg(return_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("24h Volume: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(market_volume_str, Style::default().fg(Color::Green)),
                ]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Buy at this price, get full $1 if outcome occurs",
                    Style::default().fg(Color::DarkGray),
                )]),
            ];

            let is_markets_focused = app.navigation.focused_panel == FocusedPanel::Markets;
            let market_block_style = if is_markets_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            let market_details = Paragraph::new(market_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title("Market Details")
                        .border_style(market_block_style),
                )
                .wrap(Wrap { trim: true });
            f.render_widget(market_details, chunks[1]);
        } else {
            // Event not in cache - show loading state with same 2-panel layout
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(9), // Event info panel
                    Constraint::Min(0),    // Market details panel
                ])
                .split(area);

            // Event panel with loading indicator in title
            let is_details_focused = app.navigation.focused_panel == FocusedPanel::EventDetails;
            let event_block_style = if is_details_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            let event_loading = Paragraph::new("")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title("Event (Loading...)")
                        .border_style(event_block_style),
                );
            f.render_widget(event_loading, chunks[0]);

            // Market details panel - show the opportunity info we already have
            let is_markets_focused = app.navigation.focused_panel == FocusedPanel::Markets;
            let market_block_style = if is_markets_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            let return_color = if opp.est_return >= 5.0 {
                Color::Green
            } else if opp.est_return >= 2.0 {
                Color::Yellow
            } else {
                Color::Red
            };

            let market_volume_str = if opp.volume >= 1_000_000.0 {
                format!("${:.1}M", opp.volume / 1_000_000.0)
            } else if opp.volume >= 1_000.0 {
                format!("${:.1}K", opp.volume / 1_000.0)
            } else {
                format!("${:.0}", opp.volume)
            };

            let market_lines = vec![
                Line::from(vec![
                    Span::styled("Market: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(opp.market_name.clone(), Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(
                        opp.market_status,
                        Style::default().fg(if opp.market_status == "open" {
                            Color::Green
                        } else {
                            Color::Red
                        }),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Outcome: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(
                        opp.outcome.clone(),
                        Style::default().fg(if opp.outcome == "Yes" {
                            Color::Green
                        } else {
                            Color::Red
                        }),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Price: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(
                        format!(
                            "{} ({:.2}%)",
                            format_price_cents(opp.price),
                            opp.price * 100.0
                        ),
                        Style::default().fg(Color::Cyan),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Est. Return: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(
                        format!("{:.2}%", opp.est_return),
                        Style::default()
                            .fg(return_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("24h Volume: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(market_volume_str, Style::default().fg(Color::Green)),
                ]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Buy at this price, get full $1 if outcome occurs",
                    Style::default().fg(Color::DarkGray),
                )]),
            ];

            let market_details = Paragraph::new(market_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title("Market Details")
                        .border_style(market_block_style),
                )
                .wrap(Wrap { trim: true });
            f.render_widget(market_details, chunks[1]);
        }
    } else {
        // No opportunity selected - show empty state with 2-panel layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(9), // Event info panel
                Constraint::Min(0),    // Market details panel
            ])
            .split(area);

        let event_empty = Paragraph::new("No opportunity selected")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Event"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(event_empty, chunks[0]);

        let market_empty = Paragraph::new("")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Market Details"),
            );
        f.render_widget(market_empty, chunks[1]);
    }
}

/// Render yield search results (events with yield info)
fn render_yield_search_results(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let yield_state = &app.yield_state;

    if yield_state.is_search_loading && yield_state.search_results.is_empty() {
        let loading = Paragraph::new("Searching...")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Search Results"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(loading, area);
        return;
    }

    if yield_state.search_results.is_empty() {
        let msg = if yield_state.last_searched_query.is_empty() {
            "Type to search for events...".to_string()
        } else {
            format!("No results for '{}'", yield_state.last_searched_query)
        };
        let empty = Paragraph::new(msg)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Search Results"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(empty, area);
        return;
    }

    // Calculate visible height (area height - 2 for borders - 1 for header row)
    let visible_height = (area.height as usize).saturating_sub(3);
    let total_items = yield_state.search_results.len();
    let scroll = yield_state
        .scroll
        .min(total_items.saturating_sub(visible_height.max(1)));

    let rows: Vec<Row> = yield_state
        .search_results
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(idx, result)| {
            // Look up event from cache
            let cached_event = app.get_cached_event(&result.event_slug);
            let event_title = cached_event
                .map(|e| e.title.as_str())
                .unwrap_or(&result.event_slug);
            let markets_count = cached_event.map(|e| e.markets.len()).unwrap_or(0);

            // Format yield info
            let (yield_str, yield_color) = if let Some(ref y) = result.best_yield {
                (
                    format!("{:.1}%", y.est_return),
                    if y.est_return >= 5.0 {
                        Color::Green
                    } else if y.est_return >= 2.0 {
                        Color::Yellow
                    } else {
                        Color::Red
                    },
                )
            } else {
                ("No yield".to_string(), Color::DarkGray)
            };

            // Calculate total volume from cached event
            let total_volume: f64 = cached_event
                .map(|e| {
                    e.markets
                        .iter()
                        .map(|m| m.volume_24hr.or(m.volume_total).unwrap_or(0.0))
                        .sum()
                })
                .unwrap_or(0.0);

            // Format volume
            let volume_str = if total_volume >= 1_000_000.0 {
                format!("${:.1}M", total_volume / 1_000_000.0)
            } else if total_volume >= 1_000.0 {
                format!("${:.0}K", total_volume / 1_000.0)
            } else if total_volume > 0.0 {
                format!("${:.0}", total_volume)
            } else {
                "-".to_string()
            };

            // Format end date from cached event
            let end_str = cached_event
                .and_then(|e| e.end_date.as_ref())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .map(|d| {
                    let now = Utc::now();
                    let days = (d - now).num_days();
                    if days < 0 {
                        "expired".to_string()
                    } else if days == 0 {
                        "today".to_string()
                    } else if days == 1 {
                        "1d".to_string()
                    } else if days < 30 {
                        format!("{}d", days)
                    } else {
                        format!("{}mo", days / 30)
                    }
                })
                .unwrap_or_else(|| "N/A".to_string());

            // Check if event is favorited
            let is_favorite = app.favorites_state.is_favorite(&result.event_slug);

            // Build event title with favorite icon
            let mut title_spans = Vec::new();
            if is_favorite {
                title_spans.push(Span::styled("⚑ ", Style::default().fg(Color::Magenta)));
            }
            title_spans.push(Span::styled(
                event_title.to_string(),
                Style::default().fg(Color::White),
            ));

            // Zebra striping
            let bg_color = if idx % 2 == 0 {
                Color::Reset
            } else {
                Color::Rgb(30, 30, 40)
            };

            Row::new(vec![
                Cell::from(Line::from(title_spans)),
                Cell::from(yield_str).style(Style::default().fg(yield_color)),
                Cell::from(volume_str).style(Style::default().fg(Color::Green)),
                Cell::from(format!("{}", markets_count))
                    .style(Style::default().fg(Color::Cyan)),
                Cell::from(end_str).style(Style::default().fg(Color::Magenta)),
            ])
            .style(Style::default().bg(bg_color))
        })
        .collect();

    let is_focused = app.navigation.focused_panel == FocusedPanel::EventsList;
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let title = format!(
        "Search Results ({}) - '{}'",
        yield_state.search_results.len(),
        truncate(&yield_state.last_searched_query, 20)
    );

    // Build block with optional bottom title for searching status
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(block_style);

    if yield_state.is_search_loading {
        block = block.title_bottom(Line::from(" Searching... ").centered());
    }

    let table = Table::new(
        rows,
        [
            Constraint::Fill(1),   // Event title
            Constraint::Length(9), // Yield (e.g., "No yield")
            Constraint::Length(8), // Volume
            Constraint::Length(3), // Markets count
            Constraint::Length(7), // Expires
        ],
    )
    .header(
        Row::new(vec!["Event", "Yield", "Volume", "Mkt", "Expires"])
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(0),
    )
    .block(block)
    .column_spacing(1)
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(60, 60, 80))
            .add_modifier(Modifier::BOLD),
    );

    let mut table_state = ratatui::widgets::TableState::default();
    table_state.select(Some(yield_state.selected_index.saturating_sub(scroll)));
    f.render_stateful_widget(table, area, &mut table_state);

    // Render scrollbar if needed
    if total_items > visible_height {
        let mut scrollbar_state = ScrollbarState::new(total_items)
            .position(yield_state.scroll)
            .viewport_content_length(visible_height);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut scrollbar_state,
        );
    }
}

/// Render details for a selected yield search result
fn render_yield_search_details(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let yield_state = &app.yield_state;

    if let Some(result) = yield_state.selected_search_result() {
        // Look up event from cache
        if let Some(event) = app.get_cached_event(&result.event_slug) {
            // Calculate dynamic height for event info based on content
            let event_url = format!("https://polymarket.com/event/{}", event.slug);
            let available_width = area.width.saturating_sub(8) as usize;
            let url_lines = (event_url.len() / available_width.max(1)) + 1;
            let has_tags = !event.tags.is_empty();
            let event_panel_height = 8 + url_lines as u16 + if has_tags { 1 } else { 0 };

            // Split into event info and yield details
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(event_panel_height), // Dynamic event info height
                    Constraint::Min(0),                     // Yield details
                ])
                .split(area);

            // Use shared function to build event info lines
            let event_lines = build_event_info_lines(event, false, "-", "Trades", chunks[0].width);

            let is_details_focused = app.navigation.focused_panel == FocusedPanel::EventDetails;
            let event_block_style = if is_details_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            // Build title with event name
            let title_max_width = chunks[0].width.saturating_sub(12) as usize;
            let title = format!("Event: {}", truncate(&event.title, title_max_width));

            let event_info = Paragraph::new(event_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(title)
                        .border_style(event_block_style),
                )
                .wrap(Wrap { trim: true });
            f.render_widget(event_info, chunks[0]);

            // Yield details panel
            let is_markets_focused = app.navigation.focused_panel == FocusedPanel::Markets;
            let yield_block_style = if is_markets_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            if let Some(ref y) = result.best_yield {
                let return_color = if y.est_return >= 5.0 {
                    Color::Green
                } else if y.est_return >= 2.0 {
                    Color::Yellow
                } else {
                    Color::Red
                };

                let yield_volume_str = if y.volume >= 1_000_000.0 {
                    format!("${:.1}M", y.volume / 1_000_000.0)
                } else if y.volume >= 1_000.0 {
                    format!("${:.1}K", y.volume / 1_000.0)
                } else {
                    format!("${:.0}", y.volume)
                };

                let yield_lines = vec![
                    Line::from(vec![
                        Span::styled("Market: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(y.market_name.clone(), Style::default().fg(Color::White)),
                    ]),
                    Line::from(vec![
                        Span::styled("Outcome: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(
                            y.outcome.clone(),
                            Style::default().fg(if y.outcome == "Yes" {
                                Color::Green
                            } else {
                                Color::Red
                            }),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Price: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(
                            format!("{} ({:.2}%)", format_price_cents(y.price), y.price * 100.0),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Est. Return: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(
                            format!("{:.2}%", y.est_return),
                            Style::default()
                                .fg(return_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("24h Volume: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(yield_volume_str, Style::default().fg(Color::Green)),
                    ]),
                    Line::from(""),
                    Line::from(vec![Span::styled(
                        "Buy at this price, get full $1 if outcome occurs",
                        Style::default().fg(Color::DarkGray),
                    )]),
                ];

                let yield_details = Paragraph::new(yield_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .title("Best Yield Opportunity")
                            .border_style(yield_block_style),
                    )
                    .wrap(Wrap { trim: true });
                f.render_widget(yield_details, chunks[1]);
            } else {
                let no_yield_lines = vec![
                    Line::from(""),
                    Line::from(vec![Span::styled(
                        "No high-probability outcomes found",
                        Style::default().fg(Color::DarkGray),
                    )]),
                    Line::from(""),
                    Line::from(vec![Span::styled(
                        format!(
                            "(Looking for outcomes >= {:.0}%)",
                            app.yield_state.min_prob * 100.0
                        ),
                        Style::default().fg(Color::DarkGray),
                    )]),
                ];

                let no_yield = Paragraph::new(no_yield_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .title("No Yield Opportunity")
                            .border_style(yield_block_style),
                    )
                    .alignment(Alignment::Center);
                f.render_widget(no_yield, chunks[1]);
            }
        } else {
            // Event not in cache - show loading state with same 2-panel layout
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(9), // Event info panel
                    Constraint::Min(0),    // Yield details panel
                ])
                .split(area);

            // Event panel with loading indicator in title
            let is_details_focused = app.navigation.focused_panel == FocusedPanel::EventDetails;
            let event_block_style = if is_details_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            let event_loading = Paragraph::new("")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title("Event (Loading...)")
                        .border_style(event_block_style),
                );
            f.render_widget(event_loading, chunks[0]);

            // Yield details panel - show best yield info if available
            let is_markets_focused = app.navigation.focused_panel == FocusedPanel::Markets;
            let yield_block_style = if is_markets_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            if let Some(ref y) = result.best_yield {
                let return_color = if y.est_return >= 5.0 {
                    Color::Green
                } else if y.est_return >= 2.0 {
                    Color::Yellow
                } else {
                    Color::Red
                };

                let yield_volume_str = if y.volume >= 1_000_000.0 {
                    format!("${:.1}M", y.volume / 1_000_000.0)
                } else if y.volume >= 1_000.0 {
                    format!("${:.1}K", y.volume / 1_000.0)
                } else {
                    format!("${:.0}", y.volume)
                };

                let yield_lines = vec![
                    Line::from(vec![
                        Span::styled("Market: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(y.market_name.clone(), Style::default().fg(Color::White)),
                    ]),
                    Line::from(vec![
                        Span::styled("Outcome: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(
                            y.outcome.clone(),
                            Style::default().fg(if y.outcome == "Yes" {
                                Color::Green
                            } else {
                                Color::Red
                            }),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Price: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(
                            format!("{} ({:.2}%)", format_price_cents(y.price), y.price * 100.0),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Est. Return: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(
                            format!("{:.2}%", y.est_return),
                            Style::default()
                                .fg(return_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("24h Volume: ", Style::default().fg(Color::Yellow).bold()),
                        Span::styled(yield_volume_str, Style::default().fg(Color::Green)),
                    ]),
                ];

                let yield_details = Paragraph::new(yield_lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .title("Best Yield Opportunity")
                            .border_style(yield_block_style),
                    )
                    .wrap(Wrap { trim: true });
                f.render_widget(yield_details, chunks[1]);
            } else {
                let no_yield = Paragraph::new("No yield opportunity")
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .title("Yield Details")
                            .border_style(yield_block_style),
                    )
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(no_yield, chunks[1]);
            }
        }
    } else {
        // No search result selected - show empty state with 2-panel layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(9), // Event info panel
                Constraint::Min(0),    // Yield details panel
            ])
            .split(area);

        let event_empty = Paragraph::new("Select an event to see details")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Event"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(event_empty, chunks[0]);

        let yield_empty = Paragraph::new("")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Yield Details"),
            );
        f.render_widget(yield_empty, chunks[1]);
    }
}

/// Helper function to center a rectangle
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

/// Render a dim overlay over the entire screen to indicate modal is active
fn render_dim_overlay(f: &mut Frame) {
    let area = f.area();
    // Create a block with a dim background to overlay the entire screen
    let overlay = Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0)));
    f.render_widget(overlay, area);
}

/// Render a popup/modal dialog
fn render_popup(f: &mut Frame, app: &TrendingAppState, popup: &PopupType) {
    // First, render a dim overlay to visually indicate the modal is active
    render_dim_overlay(f);

    match popup {
        PopupType::Login => {
            render_login_popup(f, app);
            return;
        },
        PopupType::UserProfile => {
            render_user_profile_popup(f, app);
            return;
        },
        PopupType::Trade => {
            render_trade_popup(f, app);
            return;
        },
        _ => {},
    }

    let area = centered_rect(60, 50, f.area());

    // Clear the area behind the popup
    f.render_widget(Clear, area);

    let (title, content) = match popup {
        PopupType::Help => (
            "Help - Keyboard Shortcuts",
            vec![
                Line::from(vec![Span::styled(
                    "Navigation:",
                    Style::default().fg(Color::Yellow).bold(),
                )]),
                Line::from("  ↑/k, ↓/j  - Move up/down in lists"),
                Line::from("  Tab       - Switch between panels"),
                Line::from("  ←/→       - Switch between tabs (Trending/Breaking/New)"),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Actions:",
                    Style::default().fg(Color::Yellow).bold(),
                )]),
                Line::from("  Enter     - Toggle watching event for live trades"),
                Line::from("  /         - API search (searches Polymarket)"),
                Line::from("  f         - Local filter (filters current list)"),
                Line::from("  Esc       - Cancel search/filter or close popup"),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Other:",
                    Style::default().fg(Color::Yellow).bold(),
                )]),
                Line::from("  ?         - Show this help"),
                Line::from("  q         - Quit"),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Press Esc to close",
                    Style::default().fg(Color::DarkGray),
                )]),
            ],
        ),
        PopupType::ConfirmQuit => (
            "Confirm Quit",
            vec![
                Line::from(""),
                Line::from("Are you sure you want to quit?"),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  y  ", Style::default().fg(Color::Green).bold()),
                    Span::styled("- Yes, quit", Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("  n  ", Style::default().fg(Color::Red).bold()),
                    Span::styled("- No, cancel", Style::default().fg(Color::White)),
                ]),
            ],
        ),
        PopupType::EventInfo(slug) => (
            "Event Info",
            vec![
                Line::from(format!("Slug: {}", slug)),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Press Esc to close",
                    Style::default().fg(Color::DarkGray),
                )]),
            ],
        ),
        // These are handled above with early return
        PopupType::Login | PopupType::UserProfile | PopupType::Trade => unreachable!(),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Helper to render an input field in the login form
fn render_login_input_field(
    f: &mut Frame,
    inner_x: u16,
    field_y: u16,
    label_width: u16,
    field_width: u16,
    label: &str,
    value: &str,
    is_active: bool,
    is_secret: bool,
) -> Option<ratatui::layout::Position> {
    use ratatui::layout::Position;

    // Render label
    let label_para =
        Paragraph::new(format!("{}:", label)).style(Style::default().fg(Color::Yellow).bold());
    let label_area = Rect {
        x: inner_x,
        y: field_y,
        width: label_width,
        height: 1,
    };
    f.render_widget(label_para, label_area);

    // Render input box with background
    let input_area = Rect {
        x: inner_x + label_width,
        y: field_y,
        width: field_width,
        height: 1,
    };

    // Display value (masked for secrets)
    let display_value = if is_secret && !value.is_empty() {
        "*".repeat(value.len().min(field_width as usize - 2))
    } else {
        value.to_string()
    };

    // Style: different background for input field, highlighted when active
    let (fg_color, bg_color) = if is_active {
        (Color::White, Color::DarkGray)
    } else {
        (Color::Gray, Color::Rgb(30, 30, 30))
    };

    // Pad the display value to fill the field width (creates visible input area)
    let padded_value = format!(
        "{:<width$}",
        display_value,
        width = field_width as usize - 1
    );

    let input_para = Paragraph::new(padded_value).style(Style::default().fg(fg_color).bg(bg_color));
    f.render_widget(input_para, input_area);

    // Return cursor position if this field is active
    if is_active {
        let cursor_x = input_area.x + display_value.len() as u16;
        Some(Position::new(cursor_x, input_area.y))
    } else {
        None
    }
}

/// Render the login popup with input fields
fn render_login_popup(f: &mut Frame, app: &TrendingAppState) {
    use ratatui::layout::Position;

    let area = centered_rect(80, 85, f.area());
    f.render_widget(Clear, area);

    let form = &app.login_form;

    // Calculate inner area (inside the popup border)
    let inner_area = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    // Field width for input boxes (leaving room for label)
    let label_width = 14u16;
    let field_width = inner_area.width.saturating_sub(label_width + 2);

    // Render the main popup block
    let block = Block::default()
        .title("Login - API Credentials")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));
    f.render_widget(block, area);

    // Header text
    let header = Paragraph::new("Enter your Polymarket API credentials:")
        .style(Style::default().fg(Color::White));
    let header_area = Rect {
        x: inner_area.x,
        y: inner_area.y + 1,
        width: inner_area.width,
        height: 1,
    };
    f.render_widget(header, header_area);

    // Track cursor position for the active field
    let mut cursor_position: Option<Position> = None;

    // Starting y position for fields
    let base_y = inner_area.y + 3;

    // Render required fields (each field takes 2 rows)
    if let Some(pos) = render_login_input_field(
        f,
        inner_area.x,
        base_y,
        label_width,
        field_width,
        "API Key",
        &form.api_key,
        form.active_field == LoginField::ApiKey,
        false,
    ) {
        cursor_position = Some(pos);
    }

    if let Some(pos) = render_login_input_field(
        f,
        inner_area.x,
        base_y + 2,
        label_width,
        field_width,
        "Secret",
        &form.secret,
        form.active_field == LoginField::Secret,
        true,
    ) {
        cursor_position = Some(pos);
    }

    if let Some(pos) = render_login_input_field(
        f,
        inner_area.x,
        base_y + 4,
        label_width,
        field_width,
        "Passphrase",
        &form.passphrase,
        form.active_field == LoginField::Passphrase,
        true,
    ) {
        cursor_position = Some(pos);
    }

    if let Some(pos) = render_login_input_field(
        f,
        inner_area.x,
        base_y + 6,
        label_width,
        field_width,
        "Address",
        &form.address,
        form.active_field == LoginField::Address,
        false,
    ) {
        cursor_position = Some(pos);
    }

    // Section header for optional cookies (after 4 fields = 8 rows + 1 gap)
    let cookie_section_y = base_y + 9;
    let cookie_header = Paragraph::new(Line::from(vec![
        Span::styled(
            "Optional: Browser Cookies ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "(for Favorites feature)",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    let cookie_header_area = Rect {
        x: inner_area.x,
        y: cookie_section_y,
        width: inner_area.width,
        height: 1,
    };
    f.render_widget(cookie_header, cookie_header_area);

    // Help text for cookies
    let cookie_help = Paragraph::new("Get from browser DevTools > Application > Cookies")
        .style(Style::default().fg(Color::DarkGray).italic());
    let cookie_help_area = Rect {
        x: inner_area.x,
        y: cookie_section_y + 1,
        width: inner_area.width,
        height: 1,
    };
    f.render_widget(cookie_help, cookie_help_area);

    // Render optional cookie fields
    let cookie_fields_y = cookie_section_y + 3;

    if let Some(pos) = render_login_input_field(
        f,
        inner_area.x,
        cookie_fields_y,
        label_width,
        field_width,
        "Session",
        &form.session_cookie,
        form.active_field == LoginField::SessionCookie,
        false,
    ) {
        cursor_position = Some(pos);
    }

    if let Some(pos) = render_login_input_field(
        f,
        inner_area.x,
        cookie_fields_y + 2,
        label_width,
        field_width,
        "Nonce",
        &form.session_nonce,
        form.active_field == LoginField::SessionNonce,
        false,
    ) {
        cursor_position = Some(pos);
    }

    if let Some(pos) = render_login_input_field(
        f,
        inner_area.x,
        cookie_fields_y + 4,
        label_width,
        field_width,
        "Auth Type",
        &form.session_auth_type,
        form.active_field == LoginField::SessionAuthType,
        false,
    ) {
        cursor_position = Some(pos);
    }

    // Error message area
    let error_y = cookie_fields_y + 7;
    if let Some(ref error) = form.error_message {
        let error_para = Paragraph::new(format!("Error: {}", error))
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: true });
        let error_area = Rect {
            x: inner_area.x,
            y: error_y,
            width: inner_area.width,
            height: 2,
        };
        f.render_widget(error_para, error_area);
    }

    // Validation status
    if form.is_validating {
        let validating_para =
            Paragraph::new("Validating credentials...").style(Style::default().fg(Color::Yellow));
        let validating_area = Rect {
            x: inner_area.x,
            y: error_y,
            width: inner_area.width,
            height: 1,
        };
        f.render_widget(validating_para, validating_area);
    }

    // Instructions at bottom
    let instructions = Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan).bold()),
        Span::styled(" Next  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Shift+Tab", Style::default().fg(Color::Cyan).bold()),
        Span::styled(" Prev  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Green).bold()),
        Span::styled(" Submit  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Red).bold()),
        Span::styled(" Cancel", Style::default().fg(Color::DarkGray)),
    ]);
    let instructions_para = Paragraph::new(instructions);
    let instructions_area = Rect {
        x: inner_area.x,
        y: area.y + area.height - 3,
        width: inner_area.width,
        height: 1,
    };
    f.render_widget(instructions_para, instructions_area);

    // Set cursor position for the active field
    if let Some(pos) = cursor_position {
        f.set_cursor_position(pos);
    }
}

/// Render user profile popup
fn render_user_profile_popup(f: &mut Frame, app: &TrendingAppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    let auth = &app.auth_state;

    let mut content = vec![Line::from("")];

    // Profile section header
    content.push(Line::from(vec![Span::styled(
        "Profile",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )]));
    content.push(Line::from(""));

    // Show profile information if available
    if let Some(ref profile) = auth.profile {
        // Name
        if let Some(ref name) = profile.name {
            content.push(Line::from(vec![
                Span::styled("Name:      ", Style::default().fg(Color::DarkGray)),
                Span::styled(name.clone(), Style::default().fg(Color::White).bold()),
            ]));
        }

        // Pseudonym (if different from name)
        if let Some(ref pseudonym) = profile.pseudonym {
            let show_pseudonym = profile
                .name
                .as_ref()
                .map(|n| n != pseudonym)
                .unwrap_or(true);
            if show_pseudonym {
                content.push(Line::from(vec![
                    Span::styled("Pseudonym: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(pseudonym.clone(), Style::default().fg(Color::Cyan)),
                ]));
            }
        }

        // Bio
        if let Some(ref bio) = profile.bio
            && !bio.is_empty()
        {
            content.push(Line::from(""));
            content.push(Line::from(vec![
                Span::styled("Bio:       ", Style::default().fg(Color::DarkGray)),
                Span::styled(truncate(bio, 50), Style::default().fg(Color::White)),
            ]));
        }

        // Profile image URL (truncated)
        if let Some(ref img) = profile.profile_image
            && !img.is_empty()
        {
            content.push(Line::from(vec![
                Span::styled("Avatar:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(truncate(img, 45), Style::default().fg(Color::Blue)),
            ]));
        }
    } else if auth.username.is_some() {
        // Fallback: just show username if we have it but no full profile
        content.push(Line::from(vec![
            Span::styled("Username:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                auth.username.clone().unwrap_or_default(),
                Style::default().fg(Color::White).bold(),
            ),
        ]));
    } else {
        content.push(Line::from(vec![Span::styled(
            "(No profile information available)",
            Style::default().fg(Color::DarkGray),
        )]));
    }

    content.push(Line::from(""));
    content.push(Line::from(vec![Span::styled(
        "─".repeat(55),
        Style::default().fg(Color::DarkGray),
    )]));

    // Account section header
    content.push(Line::from(""));
    content.push(Line::from(vec![Span::styled(
        "Account",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )]));
    content.push(Line::from(""));

    // Status
    content.push(Line::from(vec![
        Span::styled("Status:    ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            if auth.is_authenticated {
                "Authenticated"
            } else {
                "Not authenticated"
            },
            Style::default().fg(if auth.is_authenticated {
                Color::Green
            } else {
                Color::Red
            }),
        ),
    ]));

    // Address
    if let Some(ref addr) = auth.address {
        content.push(Line::from(vec![
            Span::styled("Address:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(addr.clone(), Style::default().fg(Color::Cyan)),
        ]));
    }

    // Balance (cash)
    if let Some(balance) = auth.balance {
        content.push(Line::from(vec![
            Span::styled("Cash:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("${:.2} USDC", balance),
                Style::default().fg(Color::Green),
            ),
        ]));
    }

    // Portfolio value
    if let Some(portfolio_value) = auth.portfolio_value {
        // Use abs() to avoid displaying -$0.00
        let display_value = if portfolio_value.abs() < 0.005 {
            0.0
        } else {
            portfolio_value
        };
        content.push(Line::from(vec![
            Span::styled("Portfolio: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("${:.2}", display_value),
                Style::default().fg(Color::Green),
            ),
        ]));
    }

    // Positions count
    if let Some(positions_count) = auth.positions_count {
        content.push(Line::from(vec![
            Span::styled("Positions: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", positions_count),
                Style::default().fg(Color::Cyan),
            ),
        ]));
    }

    // Total value (cash + portfolio)
    if auth.balance.is_some() || auth.portfolio_value.is_some() {
        let total = auth.balance.unwrap_or(0.0) + auth.portfolio_value.unwrap_or(0.0);
        content.push(Line::from(""));
        content.push(Line::from(vec![
            Span::styled("Total:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("${:.2}", total),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    // Profit/Loss section
    if auth.unrealized_pnl.is_some() || auth.realized_pnl.is_some() {
        content.push(Line::from(""));
        content.push(Line::from(vec![Span::styled(
            "─".repeat(55),
            Style::default().fg(Color::DarkGray),
        )]));

        content.push(Line::from(""));
        content.push(Line::from(vec![Span::styled(
            "Profit / Loss",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )]));
        content.push(Line::from(""));

        // Unrealized P&L
        if let Some(unrealized) = auth.unrealized_pnl {
            let (pnl_str, pnl_color) = format_pnl(unrealized);
            content.push(Line::from(vec![
                Span::styled("Unrealized:", Style::default().fg(Color::DarkGray)),
                Span::styled(format!(" {}", pnl_str), Style::default().fg(pnl_color)),
            ]));
        }

        // Realized P&L
        if let Some(realized) = auth.realized_pnl {
            let (pnl_str, pnl_color) = format_pnl(realized);
            content.push(Line::from(vec![
                Span::styled("Realized:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!(" {}", pnl_str), Style::default().fg(pnl_color)),
            ]));
        }

        // Total P&L
        let total_pnl = auth.unrealized_pnl.unwrap_or(0.0) + auth.realized_pnl.unwrap_or(0.0);
        let (total_pnl_str, total_pnl_color) = format_pnl(total_pnl);
        content.push(Line::from(""));
        content.push(Line::from(vec![
            Span::styled("Total P&L: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {}", total_pnl_str),
                Style::default()
                    .fg(total_pnl_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    content.push(Line::from(""));
    content.push(Line::from(vec![Span::styled(
        "─".repeat(55),
        Style::default().fg(Color::DarkGray),
    )]));
    content.push(Line::from(""));

    // Instructions
    content.push(Line::from(vec![
        Span::styled("Esc", Style::default().fg(Color::Cyan).bold()),
        Span::styled(" close    ", Style::default().fg(Color::DarkGray)),
        Span::styled("L", Style::default().fg(Color::Red).bold()),
        Span::styled(" logout", Style::default().fg(Color::DarkGray)),
    ]));

    // Build title with username if available
    let title = if let Some(ref name) = auth.username {
        format!(" {} ", name)
    } else {
        " User Profile ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Render trade popup with buy/sell form
fn render_trade_popup(f: &mut Frame, app: &TrendingAppState) {
    let area = centered_rect(65, 55, f.area());
    f.render_widget(Clear, area);

    let form = match &app.trade_form {
        Some(form) => form,
        None => {
            // Fallback if no form state
            let block = Block::default()
                .title("Trade")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Red))
                .style(Style::default().bg(Color::Black));
            let paragraph = Paragraph::new("Error: No trade form state")
                .block(block)
                .alignment(Alignment::Center);
            f.render_widget(paragraph, area);
            return;
        },
    };

    use crate::trending_tui::state::{TradeField, TradeSide};

    // Build content lines
    let mut content = vec![
        Line::from(""),
        // Market question (truncated)
        Line::from(vec![Span::styled(
            truncate(&form.market_question, 55),
            Style::default().fg(Color::White).bold(),
        )]),
        Line::from(""),
        // Outcome
        Line::from(vec![
            Span::styled("Outcome: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&form.outcome, Style::default().fg(Color::Cyan).bold()),
        ]),
        // Current price
        Line::from(vec![
            Span::styled("Price:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.0}¢", form.price * 100.0),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
    ];

    // Show balance if authenticated
    if app.auth_state.is_authenticated
        && let Some(balance) = app.auth_state.balance
    {
        content.push(Line::from(vec![
            Span::styled("Balance: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("${:.2}", balance),
                Style::default().fg(Color::Green),
            ),
        ]));
    }

    content.push(Line::from(""));
    content.push(Line::from(vec![Span::styled(
        "─".repeat(50),
        Style::default().fg(Color::DarkGray),
    )]));
    content.push(Line::from(""));

    // Side selection (BUY / SELL)
    let side_active = form.active_field == TradeField::Side;
    let buy_style = if form.side == TradeSide::Buy {
        Style::default().fg(Color::Black).bg(Color::Green).bold()
    } else if side_active {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let sell_style = if form.side == TradeSide::Sell {
        Style::default().fg(Color::Black).bg(Color::Red).bold()
    } else if side_active {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    content.push(Line::from(vec![
        Span::styled("Side:    ", Style::default().fg(Color::DarkGray)),
        Span::styled(" BUY ", buy_style),
        Span::raw("  "),
        Span::styled(" SELL ", sell_style),
        if side_active {
            Span::styled("  ← Tab to toggle", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw("")
        },
    ]));

    content.push(Line::from(""));

    // Amount input field
    let amount_active = form.active_field == TradeField::Amount;
    let amount_style = if amount_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let amount_display = if form.amount.is_empty() {
        "0.00".to_string()
    } else {
        form.amount.clone()
    };

    content.push(Line::from(vec![
        Span::styled("Amount:  ", Style::default().fg(Color::DarkGray)),
        Span::styled("$ ", amount_style),
        Span::styled(
            &amount_display,
            if amount_active {
                Style::default().fg(Color::White).bold()
            } else {
                Style::default().fg(Color::White)
            },
        ),
        if amount_active {
            Span::styled("_", Style::default().fg(Color::Cyan))
        } else {
            Span::raw("")
        },
    ]));

    content.push(Line::from(""));

    // Estimated shares and profit
    let shares = form.estimated_shares();
    let profit = form.potential_profit();

    content.push(Line::from(vec![
        Span::styled("Est. Shares: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.2}", shares), Style::default().fg(Color::White)),
    ]));

    let profit_color = if profit >= 0.0 {
        Color::Green
    } else {
        Color::Red
    };
    content.push(Line::from(vec![
        Span::styled(
            if form.side == TradeSide::Buy {
                "Potential Profit: "
            } else {
                "Proceeds: "
            },
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!(
                "{}${:.2}",
                if profit >= 0.0 {
                    "+"
                } else {
                    ""
                },
                profit.abs()
            ),
            Style::default().fg(profit_color),
        ),
        if form.side == TradeSide::Buy {
            Span::styled(" (if outcome wins)", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw("")
        },
    ]));

    content.push(Line::from(""));
    content.push(Line::from(vec![Span::styled(
        "─".repeat(50),
        Style::default().fg(Color::DarkGray),
    )]));

    // Error message if any
    if let Some(ref error) = form.error_message {
        content.push(Line::from(""));
        content.push(Line::from(vec![Span::styled(
            error,
            Style::default().fg(Color::Red),
        )]));
    }

    // Not authenticated warning
    if !app.auth_state.is_authenticated {
        content.push(Line::from(""));
        content.push(Line::from(vec![Span::styled(
            "⚠ Login required to trade",
            Style::default().fg(Color::Yellow),
        )]));
    }

    content.push(Line::from(""));

    // Instructions
    content.push(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan).bold()),
        Span::styled(" switch field  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Space", Style::default().fg(Color::Cyan).bold()),
        Span::styled(" toggle side  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Green).bold()),
        Span::styled(" submit  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Red).bold()),
        Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
    ]));

    let title = format!(" {} {} ", form.side.label(), truncate(&form.outcome, 20));
    let border_color = if form.side == TradeSide::Buy {
        Color::Green
    } else {
        Color::Red
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

fn render_events_list(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    // Show loading state when events are empty and we're fetching
    if app.events.is_empty() && app.pagination.is_fetching_more {
        let is_focused = app.navigation.focused_panel == FocusedPanel::EventsList;
        let block_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let loading_text = format!("Loading {} events...", app.event_filter.label());
        let loading = Paragraph::new(loading_text)
            .alignment(ratatui::layout::Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Events (Loading...) ")
                    .border_style(block_style),
            );
        f.render_widget(loading, area);
        return;
    }

    let filtered_events = app.filtered_events();
    let scroll = app.current_events_scroll();
    let selected_index = app.current_selected_index();
    let visible_events: Vec<_> = filtered_events
        .iter()
        .enumerate()
        .skip(scroll)
        .take(area.height as usize - 2)
        .collect();

    // First pass: calculate max width of market count for alignment
    let max_markets_width = visible_events
        .iter()
        .map(|(_, event)| event.markets.len().to_string().len())
        .max()
        .unwrap_or(1);

    let items: Vec<ListItem> = visible_events
        .into_iter()
        .map(|(idx, event)| {
            let is_selected = idx == selected_index;

            // Check if event is closed/inactive (not accepting trades)
            let is_closed = event.closed || !event.active;

            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else if is_closed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            let markets_count = event.markets.len();
            let markets_str = format!("{:>width$}", markets_count, width = max_markets_width);

            // For Breaking tab, show price change; for other tabs, show volume
            let (metric_str, metric_color) =
                if app.event_filter == crate::trending_tui::state::EventFilter::Breaking {
                    // Show price change percentage for Breaking tab
                    if let Some(price_change) = event.max_price_change_24hr {
                        let change_str = format!("{:+.0}%", price_change * 100.0);
                        let color = if price_change >= 0.0 {
                            Color::Green
                        } else {
                            Color::Red
                        };
                        (change_str, color)
                    } else {
                        (String::new(), Color::Green)
                    }
                } else {
                    // Calculate total volume from all markets for other tabs
                    let total_volume: f64 = event
                        .markets
                        .iter()
                        .map(|m| m.volume_24hr.or(m.volume_total).unwrap_or(0.0))
                        .sum();
                    let volume_str = if total_volume >= 1_000_000.0 {
                        format!("${:.1}M", total_volume / 1_000_000.0)
                    } else if total_volume >= 1_000.0 {
                        format!("${:.0}K", total_volume / 1_000.0)
                    } else if total_volume > 0.0 {
                        format!("${:.0}", total_volume)
                    } else {
                        String::new()
                    };
                    (volume_str, Color::Green)
                };
            let volume_str = metric_str;
            let volume_color = metric_color;

            // Format: "title ...spaces... [trades] volume markets" (right-aligned)
            // Account for List widget borders (2 chars) and some padding
            let usable_width = area.width.saturating_sub(2) as usize; // -2 for borders

            // Get received trade count for this event (from websocket)
            let trade_count = app.get_trades(&event.slug).len();
            let trade_count_str = if trade_count > 0 {
                format!("{} ", trade_count)
            } else {
                String::new()
            };

            // Build the right-aligned text: "[trades] volume markets"
            let right_text = if volume_str.is_empty() {
                format!("{}{}", trade_count_str, markets_str)
            } else {
                format!("{}{} {}", trade_count_str, volume_str, markets_str)
            };
            let right_text_width = right_text.width();

            // Reserve space for right text + 1 space padding + icons if needed
            let closed_icon = if is_closed {
                "✕ "
            } else {
                ""
            };
            let closed_icon_width = closed_icon.width();

            // Check for yield opportunity (high probability market)
            let has_yield = !is_closed && event_has_yield(event);
            let yield_icon = if has_yield {
                "$ "
            } else {
                ""
            };
            let yield_icon_width = yield_icon.width();

            // Check if event is favorited
            let is_favorite = app.favorites_state.is_favorite(&event.slug);
            let favorite_icon = if is_favorite {
                "⚑ "
            } else {
                ""
            };
            let favorite_icon_width = favorite_icon.width();

            let reserved_width =
                right_text_width + 1 + closed_icon_width + yield_icon_width + favorite_icon_width;
            let available_width = usable_width.saturating_sub(reserved_width);

            // Truncate title to fit available space (using display width)
            let title = truncate_to_width(&event.title, available_width);

            let title_width = title.width();
            let remaining_width = usable_width
                .saturating_sub(closed_icon_width)
                .saturating_sub(yield_icon_width)
                .saturating_sub(favorite_icon_width)
                .saturating_sub(title_width)
                .saturating_sub(right_text_width);

            let mut line_spans = Vec::new();
            if is_favorite {
                line_spans.push(Span::styled(
                    favorite_icon,
                    Style::default().fg(Color::Magenta),
                ));
            }
            if is_closed {
                line_spans.push(Span::styled(closed_icon, Style::default().fg(Color::Red)));
            }
            if has_yield {
                line_spans.push(Span::styled(yield_icon, Style::default().fg(Color::Green)));
            }
            line_spans.push(Span::styled(title, style));

            // Add spaces to right-align the markets/trades count
            if remaining_width > 0 {
                line_spans.push(Span::styled(" ".repeat(remaining_width), Style::default()));
            }

            // Add the right-aligned text with appropriate styling
            // Trade count in yellow, volume/price-change in green/red, markets in cyan
            if trade_count > 0 {
                line_spans.push(Span::styled(
                    format!("{} ", trade_count),
                    Style::default().fg(Color::Yellow),
                ));
            }
            if !volume_str.is_empty() {
                line_spans.push(Span::styled(
                    volume_str.clone(),
                    Style::default().fg(volume_color),
                ));
                line_spans.push(Span::styled(" ", Style::default()));
            }
            line_spans.push(Span::styled(markets_str, Style::default().fg(Color::Cyan)));

            // Alternating row colors (zebra striping) for better readability
            let bg_color = if idx % 2 == 0 {
                Color::Reset // Default background
            } else {
                Color::Rgb(30, 30, 40) // Slightly darker for odd rows
            };

            ListItem::new(Line::from(line_spans)).style(Style::default().bg(bg_color))
        })
        .collect();

    let is_focused = app.navigation.focused_panel == FocusedPanel::EventsList;
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    // Build title with count and search query if applicable
    let event_count = app.filtered_events().len();
    let title = if !app.search.last_searched_query.is_empty() && !app.search.results.is_empty() {
        // Show search query in title when displaying API search results
        format!(
            "Events ({}) - \"{}\"",
            event_count, app.search.last_searched_query
        )
    } else {
        format!("Events ({})", event_count)
    };

    // Build block with optional bottom title for loading status
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(block_style);

    if app.pagination.is_fetching_more {
        block = block.title_bottom(Line::from(" Loading more... ").centered());
    } else if app.search.is_searching {
        block = block.title_bottom(Line::from(" Searching... ").centered());
    }

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED),
    );

    let mut state = ListState::default();
    state.select(Some(selected_index.saturating_sub(scroll)));
    f.render_stateful_widget(list, area, &mut state);

    // Render scrollbar for events list if needed
    let total_events = filtered_events.len();
    let visible_height = (area.height as usize).saturating_sub(2);
    if total_events > visible_height {
        // ScrollbarState automatically calculates thumb size as:
        // thumb_height = (viewport_content_length / content_length) * track_height
        // This ensures the thumb is proportional to visible content
        // Position maps correctly: moving one line moves thumb proportionally
        let mut scrollbar_state = ScrollbarState::new(total_events)
            .position(scroll)
            .viewport_content_length(visible_height);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut scrollbar_state,
        );
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

        // Split area into event details, markets, and trades
        // Use Spacing::Overlap(1) to collapse borders between panels
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(min_event_details_height as u16), // Event details (minimum height, scrollable)
                Constraint::Length(7), // Markets panel (5 lines + 2 for borders)
                Constraint::Min(0),    // Trades table
            ])
            .split(area);

        // Render event details
        render_event_details(f, app, event, is_watching, trades.len(), chunks[0]);

        // Render markets panel
        render_markets(f, app, event, chunks[1]);

        // Render trades table
        if trades.is_empty() {
            let status_text = if is_watching {
                "Watching for trades... (Press Enter to stop)"
            } else {
                "Not watching. Press Enter to start watching this event."
            };
            let is_focused = app.navigation.focused_panel == FocusedPanel::Trades;
            let block_style = if is_focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            let paragraph = Paragraph::new(status_text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_type(BorderType::Rounded)
                        .title(if is_focused {
                            format!("Trades ({}) (Focused)", trades.len())
                        } else {
                            format!("Trades ({})", trades.len())
                        })
                        .border_style(block_style),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
            f.render_widget(paragraph, chunks[2]);
        } else {
            // Calculate visible rows and apply scroll
            let visible_height = (chunks[2].height as usize).saturating_sub(3); // -3 for header
            let total_rows = trades.len();
            let scroll = app
                .scroll
                .trades
                .min(total_rows.saturating_sub(visible_height.max(1)));

            let rows: Vec<Row> = trades
                .iter()
                .enumerate()
                .skip(scroll)
                .take(visible_height)
                .map(|(idx, trade)| {
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

                    // Find the market by asset_id and use short name if available
                    let market_name = event
                        .markets
                        .iter()
                        .find(|m| {
                            m.clob_token_ids
                                .as_ref()
                                .is_some_and(|ids| ids.contains(&trade.asset_id))
                        })
                        .and_then(|m| {
                            m.group_item_title
                                .as_deref()
                                .filter(|s| !s.is_empty())
                                .or(Some(m.question.as_str()))
                        })
                        .unwrap_or(&trade.title);

                    let title_truncated = truncate(market_name, 30);
                    // Use user, fall back to pseudonym, or show "-" if both empty
                    let user_display = if !trade.user.is_empty() {
                        &trade.user
                    } else if !trade.pseudonym.is_empty() {
                        &trade.pseudonym
                    } else {
                        "-"
                    };
                    let user_truncated = truncate(user_display, 15);
                    let side_text = trade.side.clone();
                    let outcome_text = trade.outcome.clone();

                    // Alternating row colors (zebra striping) for better readability
                    let bg_color = if idx % 2 == 0 {
                        Color::Reset
                    } else {
                        Color::Rgb(30, 30, 40)
                    };

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
                    .style(Style::default().bg(bg_color))
                })
                .collect();

            let table = Table::new(
                rows,
                [
                    Constraint::Length(9),  // Time
                    Constraint::Length(5),  // Side
                    Constraint::Length(4),  // Outcome
                    Constraint::Length(8),  // Price
                    Constraint::Length(9),  // Shares
                    Constraint::Length(9),  // Value
                    Constraint::Fill(1),    // Market (takes remaining space)
                    Constraint::Length(12), // User
                ],
            )
            .header(
                Row::new(vec![
                    "Time", "Side", "Out", "Price", "Shares", "Value", "Market", "User",
                ])
                .style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            )
            .block({
                let is_focused = app.navigation.focused_panel == FocusedPanel::Trades;
                let block_style = if is_focused {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_type(BorderType::Rounded)
                    .title(if is_focused {
                        format!("Trades ({}) (Focused)", trades.len())
                    } else {
                        format!("Trades ({})", trades.len())
                    })
                    .border_style(block_style)
            })
            .column_spacing(1)
            .row_highlight_style(
                Style::default()
                    .bg(Color::Rgb(60, 60, 80))
                    .add_modifier(Modifier::BOLD),
            );

            // Use TableState for proper row selection (when Trades panel is focused)
            let is_focused = app.navigation.focused_panel == FocusedPanel::Trades;
            if is_focused && !trades.is_empty() {
                // Copy the state (TableState implements Copy in ratatui 0.30)
                let mut table_state = app.trades_table_state;
                // Set selection if not already set
                if table_state.selected().is_none() {
                    table_state.select(Some(0));
                }
                f.render_stateful_widget(table, chunks[2], &mut table_state);
            } else {
                f.render_widget(table, chunks[2]);
            }

            // Render scrollbar for trades if needed
            // ScrollbarState automatically calculates proportional thumb size
            if total_rows > visible_height {
                let mut scrollbar_state = ScrollbarState::new(total_rows)
                    .position(scroll)
                    .viewport_content_length(visible_height);
                f.render_stateful_widget(
                    Scrollbar::default()
                        .orientation(ScrollbarOrientation::VerticalRight)
                        .begin_symbol(Some("↑"))
                        .end_symbol(Some("↓")),
                    chunks[2],
                    &mut scrollbar_state,
                );
            }
        }
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

fn render_event_details(
    f: &mut Frame,
    app: &TrendingAppState,
    event: &Event,
    is_watching: bool,
    ws_trade_count: usize,
    area: Rect,
) {
    // Use API trade count (your trades) if available, otherwise show websocket count
    let (trade_count_display, trade_label) =
        if let Some(&api_count) = app.event_trade_counts.get(&event.slug) {
            (format!("{}", api_count), "Your Trades")
        } else if app.has_clob_auth {
            ("...".to_string(), "Your Trades")
        } else if is_watching && ws_trade_count > 0 {
            (format!("{}", ws_trade_count), "Live Trades")
        } else {
            ("-".to_string(), "Trades")
        };
    // Calculate total volume from all markets (use 24hr volume, more reliable)
    let total_volume: f64 = event
        .markets
        .iter()
        .map(|m| m.volume_24hr.or(m.volume_total).unwrap_or(0.0))
        .sum();

    // Format end date if available
    let end_date_str = event
        .end_date
        .as_ref()
        .and_then(|date_str| {
            // Try RFC3339 parsing (handles timezone offsets and UTC)
            DateTime::parse_from_rfc3339(date_str)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
        .map(|dt| {
            // Format as relative time or absolute date
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

    // Build compact lines without blank lines (title is in panel header)
    let mut lines = vec![Line::from(vec![
        Span::styled("Slug: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(truncate(&event.slug, 60), Style::default().fg(Color::Blue)),
    ])];
    let event_url = format!("https://polymarket.com/event/{}", event.slug);
    lines.push(Line::from(vec![
        Span::styled("URL: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(event_url, Style::default().fg(Color::Cyan)),
    ]));
    lines.push(Line::from(vec![
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
                "🔴 Watching"
            } else {
                "Not Watching"
            },
            Style::default().fg(if is_watching {
                Color::Red
            } else {
                Color::Gray
            }),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Estimated End: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(end_date_str, Style::default().fg(Color::Magenta)),
    ]));
    // Format volume in short form
    let volume_str = if total_volume >= 1_000_000.0 {
        format!("${:.1}M", total_volume / 1_000_000.0)
    } else if total_volume >= 1_000.0 {
        format!("${:.1}K", total_volume / 1_000.0)
    } else {
        format!("${:.0}", total_volume)
    };
    // Build trades display with label
    let trades_spans = vec![
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
            trade_count_display.clone(),
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
    ];
    lines.push(Line::from(trades_spans));

    // Add tags - may wrap to multiple lines
    if !event.tags.is_empty() {
        let tag_labels: Vec<String> = event
            .tags
            .iter()
            .map(|tag| truncate(&tag.label, 20))
            .collect();
        let tags_text = tag_labels.join(", ");

        // Calculate available width for tags (accounting for "Tags: " prefix and borders)
        let available_width = (area.width as usize).saturating_sub(8); // "Tags: " (6) + borders (2)

        // If tags text fits on one line, add it normally
        let tags_char_count = tags_text.chars().count();
        if tags_char_count <= available_width {
            lines.push(Line::from(vec![
                Span::styled("Tags: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(tags_text, Style::default().fg(Color::Cyan)),
            ]));
        } else {
            // Tags need to wrap - split into multiple lines
            let tags_prefix = "Tags: ";
            let tags_content = &tags_text;
            let content_width = available_width.saturating_sub(tags_prefix.len());

            // First line with prefix (use character-based truncation)
            let first_line_content: String = tags_content.chars().take(content_width).collect();
            lines.push(Line::from(vec![
                Span::styled(tags_prefix, Style::default().fg(Color::Yellow).bold()),
                Span::styled(first_line_content, Style::default().fg(Color::Cyan)),
            ]));

            // Additional wrapped lines (without prefix, indented)
            let remaining_content: String = tags_content.chars().skip(content_width).collect();

            // Split remaining content into chunks that fit
            let indent = "      "; // 6 spaces to align with content after "Tags: "
            let indent_width = indent.len();
            let wrapped_width = available_width.saturating_sub(indent_width);

            for chunk in remaining_content
                .chars()
                .collect::<Vec<_>>()
                .chunks(wrapped_width)
            {
                let chunk_str: String = chunk.iter().collect();
                if !chunk_str.trim().is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(indent, Style::default()),
                        Span::styled(chunk_str, Style::default().fg(Color::Cyan)),
                    ]));
                }
            }
        }
    }

    // Calculate visible height and apply scroll
    let visible_height = (area.height as usize).saturating_sub(2); // -2 for borders
    let total_lines = lines.len();
    let scroll = app
        .scroll
        .event_details
        .min(total_lines.saturating_sub(visible_height.max(1)));

    // Apply scroll offset - show only visible lines
    let visible_lines: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(visible_height)
        .cloned()
        .collect();

    let is_focused = app.navigation.focused_panel == FocusedPanel::EventDetails;
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    // Build title with event name (truncated to fit panel width)
    // Reserve space for "Event: " prefix and borders
    let title_max_width = area.width.saturating_sub(12) as usize;
    let title = format!("Event: {}", truncate(&event.title, title_max_width));

    let paragraph = Paragraph::new(visible_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title)
                .border_style(block_style),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);

    // Render scrollbar if content exceeds visible height
    if total_lines > visible_height {
        let mut scrollbar_state = ScrollbarState::new(total_lines)
            .position(scroll)
            .viewport_content_length(visible_height);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut scrollbar_state,
        );
    }
}

fn render_markets(f: &mut Frame, app: &TrendingAppState, event: &Event, area: Rect) {
    if event.markets.is_empty() {
        let paragraph = Paragraph::new("No markets available")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Markets"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(paragraph, area);
        return;
    }

    // Calculate visible height (accounting for borders: top and bottom)
    // The List widget with borders takes 2 lines (top border + title, bottom border)
    let visible_height = (area.height as usize).saturating_sub(2);
    let total_markets = event.markets.len();

    // Calculate maximum scroll position (can't scroll past the end)
    let max_scroll = total_markets.saturating_sub(visible_height.max(1));
    // Clamp scroll position to valid range
    let scroll = app.scroll.markets.min(max_scroll);

    // Sort markets: non-closed (active) first, then closed (resolved)
    let mut sorted_markets: Vec<_> = event.markets.iter().collect();
    sorted_markets.sort_by_key(|m| m.closed);

    // Create list items for markets with scroll
    let items: Vec<ListItem> = sorted_markets
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(idx, market)| {
            // Use 24hr volume (more reliable) or fall back to total volume
            let volume = market.volume_24hr.or(market.volume_total);
            let volume_str = volume
                .map(|v| {
                    if v >= 1_000_000.0 {
                        format!("${:.1}M", v / 1_000_000.0)
                    } else if v >= 1_000.0 {
                        format!("${:.0}K", v / 1_000.0)
                    } else if v > 0.0 {
                        format!("${:.0}", v)
                    } else {
                        String::new()
                    }
                })
                .unwrap_or_default();

            // Status indicator: ● for active, ◐ for in-review, ○ for resolved
            // Add $ for yield opportunity (high probability market)
            let has_yield = market_has_yield(market);

            // Calculate yield return if there's a yield opportunity
            // Find the highest price outcome that qualifies as yield (>= 95%)
            let yield_return: Option<f64> = if has_yield {
                market
                    .outcome_prices
                    .iter()
                    .filter_map(|price_str| price_str.parse::<f64>().ok())
                    .filter(|&price| (YIELD_MIN_PROB..1.0).contains(&price))
                    .map(|price| (1.0 / price - 1.0) * 100.0) // Convert to percentage return
                    .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)) // Best (lowest cost = highest price) yield
            } else {
                None
            };

            let status_icon = if market.closed {
                "○ "
            } else if has_yield {
                "$ " // Yield opportunity indicator
            } else if market.is_in_review() {
                "◐ "
            } else {
                "● "
            };

            // Build outcome display string
            let outcomes_str = if market.closed {
                // For resolved markets, show only the winning side
                // Find the outcome with highest price (closest to 1.0)
                let winner = market
                    .outcomes
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, outcome)| {
                        let price = market
                            .outcome_prices
                            .get(idx)
                            .and_then(|p| p.parse::<f64>().ok())?;
                        Some((outcome, price))
                    })
                    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                winner
                    .map(|(outcome, _)| format!("Winner: {}", outcome))
                    .unwrap_or_else(|| "Resolved".to_string())
            } else {
                // For active markets, show all outcomes with prices
                let mut outcome_strings = Vec::new();
                for (idx, outcome) in market.outcomes.iter().enumerate() {
                    let price = if let Some(ref token_ids) = market.clob_token_ids {
                        // Active markets: try live prices first, fallback to outcome_prices
                        token_ids
                            .get(idx)
                            .and_then(|asset_id| app.market_prices.get(asset_id).copied())
                            .or_else(|| {
                                market
                                    .outcome_prices
                                    .get(idx)
                                    .and_then(|p| p.parse::<f64>().ok())
                            })
                    } else {
                        // No token IDs: use outcome_prices
                        market
                            .outcome_prices
                            .get(idx)
                            .and_then(|p| p.parse::<f64>().ok())
                    };

                    let price_str = price
                        .map(format_price_cents)
                        .unwrap_or_else(|| "N/A".to_string());

                    outcome_strings.push(format!("{}: {}", outcome, price_str));
                }
                outcome_strings.join(" | ")
            };

            // Calculate widths for right alignment
            let usable_width = (area.width as usize).saturating_sub(2); // -2 for borders

            // Format yield return string if applicable
            let yield_str = yield_return.map(|ret| format!("+{:.1}%", ret));

            // Calculate space needed for right-aligned content
            // Order from right to left: volume, outcomes, yield (if any)
            // Use .width() for proper Unicode width calculation
            let outcomes_width = outcomes_str.width();
            let volume_width = volume_str.len();
            let yield_width = yield_str.as_ref().map(|s| s.len()).unwrap_or(0);

            let has_outcomes = !outcomes_str.is_empty();
            let has_volume = !volume_str.is_empty();
            let has_yield_str = yield_str.is_some();

            // Calculate total right content width: [yield] [outcomes] [volume]
            let mut right_content_width = 0;
            if has_yield_str {
                right_content_width += yield_width;
            }
            if has_outcomes {
                if right_content_width > 0 {
                    right_content_width += 1; // space
                }
                right_content_width += outcomes_width;
            }
            if has_volume {
                if right_content_width > 0 {
                    right_content_width += 1; // space
                }
                right_content_width += volume_width;
            }

            // Calculate available width for question (reserve space for icon + right content + 1 space padding)
            let icon_width = status_icon.width();
            let available_width = usable_width
                .saturating_sub(right_content_width)
                .saturating_sub(icon_width)
                .saturating_sub(1); // 1 space padding between question and right content

            // Truncate question to fit available width
            // Use short name (group_item_title) if available and non-empty, otherwise fall back to question
            let display_name = market
                .group_item_title
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&market.question);
            let question = truncate_to_width(display_name, available_width);
            let question_width = question.width();

            // Calculate remaining width for spacing
            let remaining_width = usable_width
                .saturating_sub(icon_width)
                .saturating_sub(question_width)
                .saturating_sub(right_content_width);

            // Start with status icon
            let icon_color = if market.closed {
                Color::DarkGray
            } else if has_yield {
                Color::Green // Yield opportunity in green
            } else if market.is_in_review() {
                Color::Cyan
            } else {
                Color::Green
            };
            let mut line_spans = vec![
                Span::styled(status_icon, Style::default().fg(icon_color)),
                Span::styled(question, Style::default().fg(Color::White)),
            ];

            // Add spaces to push right content to the right
            if remaining_width > 0 {
                line_spans.push(Span::styled(" ".repeat(remaining_width), Style::default()));
            }

            // Add right-aligned content in order: [yield] [outcomes] [volume]
            // Yield return (if any) - prepended before outcomes
            if let Some(ref yield_s) = yield_str {
                line_spans.push(Span::styled(
                    yield_s.clone(),
                    Style::default().fg(Color::Yellow),
                ));
                if has_outcomes || has_volume {
                    line_spans.push(Span::styled(" ", Style::default()));
                }
            }

            // Outcomes (Yes/No prices)
            if has_outcomes {
                line_spans.push(Span::styled(
                    outcomes_str.clone(),
                    Style::default().fg(Color::Cyan),
                ));
                if has_volume {
                    line_spans.push(Span::styled(" ", Style::default()));
                }
            }

            // Volume (always on the right)
            if has_volume {
                line_spans.push(Span::styled(
                    volume_str.clone(),
                    Style::default().fg(Color::Green),
                ));
            }

            // Alternating row colors (zebra striping) for better readability
            let bg_color = if idx % 2 == 0 {
                Color::Reset
            } else {
                Color::Rgb(30, 30, 40)
            };

            ListItem::new(Line::from(line_spans)).style(Style::default().bg(bg_color))
        })
        .collect();

    let is_focused = app.navigation.focused_panel == FocusedPanel::Markets;
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(if is_focused {
                format!("Markets ({}) (Focused)", event.markets.len())
            } else {
                format!("Markets ({})", event.markets.len())
            })
            .border_style(block_style),
    );

    f.render_widget(list, area);

    // Render scrollbar if needed
    // The scrollbar thumb size is: (visible_height / total_markets) * track_height
    // This ensures proportional thumb that moves correctly when scrolling
    if total_markets > visible_height {
        // Calculate the scrollable range (max scroll position)
        let max_scroll = total_markets.saturating_sub(visible_height);
        // Ensure scroll position is within valid bounds
        let clamped_scroll = scroll.min(max_scroll);

        // ScrollbarState calculates thumb size as: (viewport_content_length / content_length) * track_height
        // content_length = total_markets (total number of items, set in new())
        // viewport_content_length = visible_height (how many items fit in viewport)
        // position = clamped_scroll (current scroll offset)
        let mut scrollbar_state = ScrollbarState::new(total_markets)
            .position(clamped_scroll)
            .viewport_content_length(visible_height);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut scrollbar_state,
        );
    }
}

pub fn truncate(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Format a profit/loss value with appropriate sign and color
/// Returns (formatted_string, color)
fn format_pnl(value: f64) -> (String, Color) {
    // Treat near-zero values as zero to avoid -$0.00
    if value.abs() < 0.005 {
        ("$0.00".to_string(), Color::DarkGray)
    } else if value > 0.0 {
        (format!("+${:.2}", value), Color::Green)
    } else {
        (format!("-${:.2}", value.abs()), Color::Red)
    }
}

/// Truncate a string to fit within a maximum display width (not byte length).
/// This properly handles Unicode characters that may have different display widths.
fn truncate_to_width(s: &str, max_width: usize) -> String {
    let current_width = s.width();
    if current_width <= max_width {
        return s.to_string();
    }

    // Need to truncate - account for "…" which is 1 column wide
    let target_width = max_width.saturating_sub(1);
    let mut result = String::new();
    let mut width = 0;

    for c in s.chars() {
        let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if width + char_width > target_width {
            break;
        }
        result.push(c);
        width += char_width;
    }

    result.push('…');
    result
}

fn render_logs(f: &mut Frame, app: &mut TrendingAppState, area: Rect) {
    // Calculate the actual visible height (accounting for borders)
    let visible_height = (area.height as usize).saturating_sub(2);

    // Auto-scroll to bottom only if Logs panel is NOT focused
    // When focused, user controls scrolling manually
    let is_focused = app.navigation.focused_panel == FocusedPanel::Logs;

    if !is_focused {
        // Auto-scroll to bottom if we're near the bottom or if logs have grown
        // This ensures new logs are always visible when panel is not focused
        if app.logs.messages.len() > visible_height {
            // Check if we're already showing the bottom (within 1 line)
            let current_bottom = app.logs.scroll + visible_height;
            if current_bottom >= app.logs.messages.len().saturating_sub(1) {
                // We're at or near the bottom, keep it there
                app.logs.scroll = app.logs.messages.len() - visible_height;
            }
        } else {
            // Not enough logs to scroll, show from the beginning
            app.logs.scroll = 0;
        }
    } else {
        // When focused, ensure scroll position is within valid bounds
        let max_scroll = app
            .logs
            .messages
            .len()
            .saturating_sub(visible_height.max(1));
        app.logs.scroll = app.logs.scroll.min(max_scroll);
    }

    // First, flatten logs by wrapping long lines
    let max_width = (area.width as usize).saturating_sub(2); // Account for borders
    let wrapped_logs: Vec<String> = app
        .logs
        .messages
        .iter()
        .skip(app.logs.scroll)
        .flat_map(|log| {
            // Split long lines by wrapping them to fit the available width
            if log.len() > max_width {
                // Split into multiple lines
                log.chars()
                    .collect::<Vec<_>>()
                    .chunks(max_width)
                    .map(|chunk| chunk.iter().collect::<String>())
                    .collect::<Vec<_>>()
            } else {
                vec![log.clone()]
            }
        })
        .take(visible_height)
        .collect();

    let log_items: Vec<ListItem> = wrapped_logs
        .iter()
        .map(|log| {
            let color = if log.starts_with("[WARN]") {
                Color::Yellow
            } else if log.starts_with("[ERROR]") {
                Color::Red
            } else {
                Color::Gray
            };
            ListItem::new(log.as_str()).style(Style::default().fg(color))
        })
        .collect();
    let is_focused = app.navigation.focused_panel == FocusedPanel::Logs;
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let logs_list = List::new(log_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(if is_focused {
                    "Logs (Focused)"
                } else {
                    "Logs"
                })
                .border_style(block_style),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(logs_list, area);

    // Render scrollbar for logs if needed
    // Note: We scroll by message count, but display wrapped lines
    // The scrollbar represents message positions, and thumb size is proportional to visible messages
    let total_log_messages = app.logs.messages.len();
    if total_log_messages > 0 {
        // Estimate visible messages based on visible height and average wrapping
        // This is approximate but ensures the scrollbar thumb is reasonably proportional
        let estimated_visible_messages = visible_height.max(1);
        let mut scrollbar_state = ScrollbarState::new(total_log_messages)
            .position(app.logs.scroll)
            .viewport_content_length(estimated_visible_messages);
        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut scrollbar_state,
        );
    }
}
