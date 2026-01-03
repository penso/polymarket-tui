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
        prelude::Stylize,
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
    Trending,
    Breaking,
    New,
    Yield,
}

/// Check if a click is on a tab (unified single line of tabs)
/// Check if the login button was clicked (top right, "[ Login ]" = 10 chars)
/// Returns true if click is on the login button area
pub fn is_login_button_clicked(x: u16, y: u16, size: Rect) -> bool {
    // Login button is on the first line (y = 0) and at the right edge
    // "[ Login ]" is 10 characters wide
    if y != 0 {
        return false;
    }

    let login_button_start = size.width.saturating_sub(10);
    x >= login_button_start
}

/// Tabs are rendered on the first line (y = 0)
/// Returns which tab was clicked: Trending [1], Breaking [2], New [3], Yield [4]
pub fn get_clicked_tab(x: u16, y: u16, size: Rect) -> Option<ClickedTab> {
    // Tabs are on the first line (y = 0)
    if y != 0 {
        return None;
    }

    // Don't match tabs if clicking on login button area (right side)
    let login_button_start = size.width.saturating_sub(10);
    if x >= login_button_start {
        return None;
    }

    // Actual rendered output (Tabs widget adds leading space and "   " divider):
    // " Trending [1]   Breaking [2]   New [3]   Yield [4]"
    // 01234567890123456789012345678901234567890123456789
    //  Trending [1]   Breaking [2]   New [3]   Yield [4]
    // Positions: 1-12 = Trending, 16-27 = Breaking, 31-36 = New, 40-48 = Yield
    if x <= 12 {
        return Some(ClickedTab::Trending);
    } else if (16..28).contains(&x) {
        return Some(ClickedTab::Breaking);
    } else if (31..37).contains(&x) {
        return Some(ClickedTab::New);
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
        format!("{} | l: Logs | q: Quit | [{}]", panel_help, panel_name)
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
    // Calculate unified tab index: 0=Trending, 1=Breaking, 2=New, 3=Yield
    let tab_index = match app.main_tab {
        MainTab::Trending => match app.event_filter {
            EventFilter::Trending => 0,
            EventFilter::Breaking => 1,
            EventFilter::New => 2,
        },
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
            Line::from("Trending [1]"),
            Line::from("Breaking [2]"),
            Line::from("New [3]"),
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

        // Search input field
        let search_line = if app.search.query.is_empty() {
            let prompt_text = match app.search.mode {
                SearchMode::ApiSearch => "Type to search via API...",
                SearchMode::LocalFilter => "Type to filter current list...",
                SearchMode::None => "Type to search...",
            };
            Line::from(prompt_text.fg(Color::DarkGray))
        } else if app.search.is_searching {
            Line::from(vec![
                Span::styled(
                    app.search.query.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" (searching...)", Style::default().fg(Color::Yellow)),
            ])
        } else {
            Line::from(Span::styled(
                app.search.query.clone(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
        };
        let search_title = if app.search.is_searching {
            "Search (loading...)"
        } else {
            "Search"
        };
        let search_input = Paragraph::new(vec![search_line])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(search_title),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });
        f.render_widget(search_input, header_chunks[2]);
    } else {
        // Normal mode: Split header into tabs and horizontal line
        let header_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tabs line
                Constraint::Length(1), // Horizontal separator line
            ])
            .split(area);

        // Split tabs line: tabs on left, login button on right
        let tabs_line_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),     // Tabs (fill remaining space)
                Constraint::Length(10), // Login button "[ Login ]"
            ])
            .split(header_chunks[0]);

        // Render unified tabs in gitui-style (underline for selected, keyboard shortcuts)
        let tab_titles: Vec<Line> = vec![
            Line::from("Trending [1]"),
            Line::from("Breaking [2]"),
            Line::from("New [3]"),
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

        // Render login/user button on the right
        let (button_text, button_style) = if app.auth_state.is_authenticated {
            let name = app.auth_state.display_name();
            // Truncate to fit in button area (max ~8 chars to fit in Length(10))
            let display = if name.len() > 8 {
                format!("{}...", &name[..5])
            } else {
                name
            };
            (
                format!("[ {} ]", display),
                Style::default().fg(Color::Green),
            )
        } else {
            ("[ Login ]".to_string(), Style::default().fg(Color::Cyan))
        };
        let login_button = Paragraph::new(button_text)
            .style(button_style)
            .alignment(Alignment::Right);
        f.render_widget(login_button, tabs_line_chunks[1]);

        // Horizontal separator line (gitui-style) - full width line of ─ characters
        let line_width = header_chunks[1].width as usize;
        let separator_line = "─".repeat(line_width);
        let separator = Paragraph::new(separator_line).style(Style::default().fg(Color::DarkGray));
        f.render_widget(separator, header_chunks[1]);
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

        // Render search input
        let search_text = if yield_state.search_query.is_empty() {
            Line::from("Type to search events...".fg(Color::DarkGray))
        } else if yield_state.is_search_loading {
            Line::from(vec![
                Span::styled(
                    &yield_state.search_query,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" (searching...)", Style::default().fg(Color::Yellow)),
            ])
        } else {
            Line::from(Span::styled(
                &yield_state.search_query,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
        };
        let search_input = Paragraph::new(vec![search_text])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Search (Esc to close)")
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .alignment(Alignment::Left);
        f.render_widget(search_input, chunks[0]);

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

        // Render filter input
        let filter_text = if yield_state.filter_query.is_empty() {
            Line::from("Type to filter by event/market name...".fg(Color::DarkGray))
        } else {
            Line::from(Span::styled(
                &yield_state.filter_query,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
        };
        let filter_input = Paragraph::new(vec![filter_text])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Filter (Esc to close, Backspace to clear)")
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .alignment(Alignment::Left);
        f.render_widget(filter_input, chunks[0]);

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

            // Format end date
            let end_str = opp
                .end_date
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

            // Create a cell with event title (dimmed) and market name
            // Let the table handle truncation based on column width
            let name_cell = Cell::from(Line::from(vec![
                Span::styled(
                    opp.event_title.as_str(),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(" > ", Style::default().fg(Color::DarkGray)),
                Span::styled(opp.market_name.as_str(), Style::default().fg(Color::White)),
            ]));

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

    let table = Table::new(rows, [
        Constraint::Fill(1),   // Market name (takes remaining space)
        Constraint::Length(7), // Return (e.g., "12.34%")
        Constraint::Length(7), // Price (e.g., "95.5¢")
        Constraint::Length(8), // Volume (e.g., "$123.4K")
        Constraint::Length(7), // Expires (e.g., "expired")
    ])
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
        // Split into event info and market details
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10), // Event info
                Constraint::Min(0),     // Market details
            ])
            .split(area);

        // Event info panel
        let end_date_str = opp
            .end_date
            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "N/A".to_string());

        let event_url = format!("https://polymarket.com/event/{}", opp.event_slug);

        let event_lines = vec![
            Line::from(vec![
                Span::styled("Event: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(
                    truncate(&opp.event_title, 50),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(
                    opp.event_status,
                    Style::default().fg(if opp.event_status == "active" {
                        Color::Green
                    } else {
                        Color::Red
                    }),
                ),
            ]),
            Line::from(vec![
                Span::styled("Ends: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(end_date_str, Style::default().fg(Color::Magenta)),
            ]),
            Line::from(vec![
                Span::styled("URL: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(event_url, Style::default().fg(Color::Cyan)),
            ]),
        ];

        let is_details_focused = app.navigation.focused_panel == FocusedPanel::EventDetails;
        let event_block_style = if is_details_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let event_info = Paragraph::new(event_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_type(BorderType::Rounded)
                    .title("Event")
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

        let volume_str = if opp.volume >= 1_000_000.0 {
            format!("${:.1}M", opp.volume / 1_000_000.0)
        } else if opp.volume >= 1_000.0 {
            format!("${:.1}K", opp.volume / 1_000.0)
        } else {
            format!("${:.0}", opp.volume)
        };

        let market_lines = vec![
            Line::from(vec![
                Span::styled("Market: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(&opp.market_name, Style::default().fg(Color::White)),
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
                    &opp.outcome,
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
                Span::styled(volume_str, Style::default().fg(Color::Green)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "💡 Buy at this price, get full $1 if outcome occurs",
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
        let empty = Paragraph::new("No opportunity selected")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Details"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(empty, area);
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

            // Format volume
            let volume_str = if result.total_volume >= 1_000_000.0 {
                format!("${:.1}M", result.total_volume / 1_000_000.0)
            } else if result.total_volume >= 1_000.0 {
                format!("${:.0}K", result.total_volume / 1_000.0)
            } else if result.total_volume > 0.0 {
                format!("${:.0}", result.total_volume)
            } else {
                "-".to_string()
            };

            // Format end date
            let end_str = result
                .end_date
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

            // Zebra striping
            let bg_color = if idx % 2 == 0 {
                Color::Reset
            } else {
                Color::Rgb(30, 30, 40)
            };

            Row::new(vec![
                Cell::from(result.event_title.as_str()).style(Style::default().fg(Color::White)),
                Cell::from(yield_str).style(Style::default().fg(yield_color)),
                Cell::from(volume_str).style(Style::default().fg(Color::Green)),
                Cell::from(format!("{}", result.markets_count))
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

    let table = Table::new(rows, [
        Constraint::Fill(1),   // Event title
        Constraint::Length(9), // Yield (e.g., "No yield")
        Constraint::Length(8), // Volume
        Constraint::Length(3), // Markets count
        Constraint::Length(7), // Expires
    ])
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
        // Split into event info and yield details
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10), // Event info
                Constraint::Min(0),     // Yield details
            ])
            .split(area);

        // Event info panel
        let end_date_str = result
            .end_date
            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "N/A".to_string());

        let event_url = format!("https://polymarket.com/event/{}", result.event_slug);

        let volume_str = if result.total_volume >= 1_000_000.0 {
            format!("${:.1}M", result.total_volume / 1_000_000.0)
        } else if result.total_volume >= 1_000.0 {
            format!("${:.1}K", result.total_volume / 1_000.0)
        } else {
            format!("${:.0}", result.total_volume)
        };

        let event_lines = vec![
            Line::from(vec![
                Span::styled("Event: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(
                    truncate(&result.event_title, 50),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(
                    result.event_status,
                    Style::default().fg(if result.event_status == "active" {
                        Color::Green
                    } else {
                        Color::Red
                    }),
                ),
            ]),
            Line::from(vec![
                Span::styled("Markets: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(
                    format!("{}", result.markets_count),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(" | Volume: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(volume_str, Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::styled("Ends: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(end_date_str, Style::default().fg(Color::Magenta)),
            ]),
            Line::from(vec![
                Span::styled("URL: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(event_url, Style::default().fg(Color::Cyan)),
            ]),
        ];

        let is_details_focused = app.navigation.focused_panel == FocusedPanel::EventDetails;
        let event_block_style = if is_details_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let event_info = Paragraph::new(event_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Event")
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
                    Span::styled(&y.market_name, Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("Outcome: ", Style::default().fg(Color::Yellow).bold()),
                    Span::styled(
                        &y.outcome,
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
                    "💡 Buy at this price, get full $1 if outcome occurs",
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
                        "(Looking for outcomes ≥{:.0}%)",
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
        let empty = Paragraph::new("Select an event to see details")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Details"),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(empty, area);
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

/// Render a popup/modal dialog
fn render_popup(f: &mut Frame, app: &TrendingAppState, popup: &PopupType) {
    match popup {
        PopupType::Login => {
            render_login_popup(f, app);
            return;
        },
        PopupType::UserProfile => {
            render_user_profile_popup(f, app);
            return;
        },
        PopupType::Trade(token_id) => {
            render_trade_popup(f, app, token_id);
            return;
        },
        _ => {},
    }

    let area = centered_rect(60, 50, f.area());

    // Clear the area behind the popup
    f.render_widget(Clear, area);

    let (title, content) = match popup {
        PopupType::Help => ("Help - Keyboard Shortcuts", vec![
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
        ]),
        PopupType::ConfirmQuit => ("Confirm Quit", vec![
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
        ]),
        PopupType::EventInfo(slug) => ("Event Info", vec![
            Line::from(format!("Slug: {}", slug)),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Press Esc to close",
                Style::default().fg(Color::DarkGray),
            )]),
        ]),
        // These are handled above with early return
        PopupType::Login | PopupType::UserProfile | PopupType::Trade(_) => unreachable!(),
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

/// Render the login popup with input fields
fn render_login_popup(f: &mut Frame, app: &TrendingAppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    let form = &app.login_form;

    // Helper to create a field line with label and value
    let make_field_line = |label: &str, value: &str, is_active: bool, is_secret: bool| -> Line {
        let display_value = if is_secret && !value.is_empty() {
            "*".repeat(value.len().min(40))
        } else if value.is_empty() {
            "(empty)".to_string()
        } else {
            value.to_string()
        };

        let label_style = Style::default().fg(Color::Yellow).bold();
        let value_style = if is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if value.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        let cursor = if is_active {
            "_"
        } else {
            ""
        };

        Line::from(vec![
            Span::styled(format!("{}: ", label), label_style),
            Span::styled(display_value, value_style),
            Span::styled(cursor, Style::default().fg(Color::Cyan)),
        ])
    };

    let mut content = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Enter your Polymarket API credentials:",
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        make_field_line(
            "API Key    ",
            &form.api_key,
            form.active_field == LoginField::ApiKey,
            false,
        ),
        Line::from(""),
        make_field_line(
            "Secret     ",
            &form.secret,
            form.active_field == LoginField::Secret,
            true,
        ),
        Line::from(""),
        make_field_line(
            "Passphrase ",
            &form.passphrase,
            form.active_field == LoginField::Passphrase,
            true,
        ),
        Line::from(""),
        make_field_line(
            "Address    ",
            &form.address,
            form.active_field == LoginField::Address,
            false,
        ),
        Line::from(""),
    ];

    // Add error message if present
    if let Some(ref error) = form.error_message {
        content.push(Line::from(vec![Span::styled(
            format!("Error: {}", error),
            Style::default().fg(Color::Red),
        )]));
        content.push(Line::from(""));
    }

    // Add validation status
    if form.is_validating {
        content.push(Line::from(vec![Span::styled(
            "Validating credentials...",
            Style::default().fg(Color::Yellow),
        )]));
    }

    content.push(Line::from(""));
    content.push(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan).bold()),
        Span::styled(" - Next field  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Green).bold()),
        Span::styled(" - Submit  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Red).bold()),
        Span::styled(" - Cancel", Style::default().fg(Color::DarkGray)),
    ]));

    let block = Block::default()
        .title("Login - API Credentials")
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

/// Render user profile popup
fn render_user_profile_popup(f: &mut Frame, app: &TrendingAppState) {
    let area = centered_rect(60, 50, f.area());
    f.render_widget(Clear, area);

    let auth = &app.auth_state;

    let mut content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Yellow).bold()),
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
        ]),
        Line::from(""),
    ];

    if let Some(ref addr) = auth.address {
        content.push(Line::from(vec![
            Span::styled("Address: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(addr.clone(), Style::default().fg(Color::Cyan)),
        ]));
    }

    if let Some(balance) = auth.balance {
        content.push(Line::from(vec![
            Span::styled("Balance: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(
                format!("${:.2} USDC", balance),
                Style::default().fg(Color::Green),
            ),
        ]));
    }

    content.push(Line::from(""));
    content.push(Line::from(vec![Span::styled(
        "Press Esc to close, or 'L' to logout",
        Style::default().fg(Color::DarkGray),
    )]));

    let block = Block::default()
        .title("User Profile")
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

/// Render trade popup (placeholder for now)
fn render_trade_popup(f: &mut Frame, _app: &TrendingAppState, token_id: &str) {
    let area = centered_rect(60, 50, f.area());
    f.render_widget(Clear, area);

    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Token ID: ", Style::default().fg(Color::Yellow).bold()),
            Span::styled(truncate(token_id, 40), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from("Trade functionality coming soon..."),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press Esc to close",
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    let block = Block::default()
        .title("Trade")
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

fn render_events_list(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let filtered_events = app.filtered_events();
    let visible_events: Vec<_> = filtered_events
        .iter()
        .enumerate()
        .skip(app.scroll.events_list)
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
            let is_selected = idx == app.navigation.selected_index;

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

            // Calculate total volume from all markets
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

            let reserved_width = right_text_width + 1 + closed_icon_width + yield_icon_width;
            let available_width = usable_width.saturating_sub(reserved_width);

            // Truncate title to fit available space (using display width)
            let title = truncate_to_width(&event.title, available_width);

            let title_width = title.width();
            let remaining_width = usable_width
                .saturating_sub(closed_icon_width)
                .saturating_sub(yield_icon_width)
                .saturating_sub(title_width)
                .saturating_sub(right_text_width);

            let mut line_spans = Vec::new();
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
            // Trade count in yellow, volume in green, markets in cyan
            if trade_count > 0 {
                line_spans.push(Span::styled(
                    format!("{} ", trade_count),
                    Style::default().fg(Color::Yellow),
                ));
            }
            if !volume_str.is_empty() {
                line_spans.push(Span::styled(
                    volume_str.clone(),
                    Style::default().fg(Color::Green),
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

    // Build title with count
    let event_count = app.filtered_events().len();
    let title = format!("Events ({})", event_count);

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
    state.select(Some(
        app.navigation
            .selected_index
            .saturating_sub(app.scroll.events_list),
    ));
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
            .position(app.scroll.events_list)
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

            let table = Table::new(rows, [
                Constraint::Length(9),  // Time
                Constraint::Length(5),  // Side
                Constraint::Length(4),  // Outcome
                Constraint::Length(8),  // Price
                Constraint::Length(9),  // Shares
                Constraint::Length(9),  // Value
                Constraint::Fill(1),    // Market (takes remaining space)
                Constraint::Length(12), // User
            ])
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

            // Calculate space needed for outcomes and volume (right-aligned)
            // Use .width() for proper Unicode width calculation
            let outcomes_width = outcomes_str.width();
            let volume_width = volume_str.len();
            let has_outcomes = !outcomes_str.is_empty();
            let has_volume = !volume_str.is_empty();
            let right_content_width = if has_outcomes && has_volume {
                outcomes_width + 1 + volume_width // outcomes + space + volume
            } else if has_outcomes {
                outcomes_width
            } else if has_volume {
                volume_width
            } else {
                0
            };

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

            // Add spaces to push outcomes and volume to the right
            if remaining_width > 0 {
                line_spans.push(Span::styled(" ".repeat(remaining_width), Style::default()));
            }

            // Add right-aligned outcomes and volume
            if has_outcomes {
                line_spans.push(Span::styled(
                    outcomes_str.clone(),
                    Style::default().fg(Color::Cyan),
                ));
            }
            if has_volume {
                if has_outcomes {
                    line_spans.push(Span::styled(" ", Style::default()));
                }
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
