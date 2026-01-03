//! Yield tab rendering functions

use {
    super::utils::{format_price_cents, truncate},
    crate::trending_tui::state::{FocusedPanel, TrendingAppState},
    chrono::{DateTime, Utc},
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{
            Block, BorderType, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation,
            ScrollbarState, Table, Wrap,
        },
    },
};

// Re-use functions from parent module
use super::{build_event_info_lines, render_search_input};

pub fn render_yield_tab(f: &mut Frame, app: &TrendingAppState, area: Rect) {
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

        // Show search results if available (even when search input is hidden), otherwise show normal yield list
        if !yield_state.search_results.is_empty() || !yield_state.last_searched_query.is_empty() {
            render_yield_search_results(f, app, chunks[0]);
            render_yield_search_details(f, app, chunks[1]);
        } else {
            render_yield_list(f, app, chunks[0]);
            render_yield_details(f, app, chunks[1]);
        }
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

    // Build title with filter info if active (count moved to bottom)
    let title = if !yield_state.filter_query.is_empty() {
        format!(
            "Yield - Filter: '{}' - Sort: {}",
            truncate(&yield_state.filter_query, 15),
            yield_state.sort_by.label()
        )
    } else {
        format!(
            "Yield Opportunities - Sort: {}",
            yield_state.sort_by.label()
        )
    };

    // Build position indicator for bottom right (lazygit style)
    let total_count = if !yield_state.filter_query.is_empty() {
        filtered.len()
    } else {
        yield_state.opportunities.len()
    };
    let position_indicator = if total_count > 0 {
        format!("{} of {}", yield_state.selected_index + 1, total_count)
    } else {
        "0 of 0".to_string()
    };

    // Build block with optional bottom title for loading/searching status
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(block_style);

    if yield_state.is_search_loading {
        block = block.title_bottom(Line::from(vec![
            Span::raw(" Searching... "),
            Span::raw(" ".repeat(10)), // spacer
            Span::raw(format!("{}─", position_indicator)),
        ]));
    } else {
        block = block.title_bottom(Line::from(format!("{}─", position_indicator)).right_aligned());
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
        // Look up the event from the global cache
        if let Some(event) = app.get_cached_event(&opp.event_slug) {
            // Calculate dynamic height for event info based on content
            // Base: 5 lines (Slug, URL, Status, End, Volume) + 2 for borders + 1 for title
            // Add 1 if tags present, add extra lines for URL wrapping
            let event_url = format!("https://polymarket.com/event/{}", event.slug);
            let available_width = area.width.saturating_sub(8) as usize; // Account for borders and "URL: " prefix
            let url_lines = (event_url.len() / available_width.max(1)) + 1;
            let has_tags = !event.tags.is_empty();
            let event_panel_height = 8
                + url_lines as u16
                + if has_tags {
                    1
                } else {
                    0
                };

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
                    "Yield Calculation:",
                    Style::default().fg(Color::Yellow).bold(),
                )]),
                Line::from(vec![Span::styled(
                    format!(
                        "  Buy at {} -> Get $1.00 if {} wins",
                        format_price_cents(opp.price),
                        opp.outcome
                    ),
                    Style::default().fg(Color::DarkGray),
                )]),
                Line::from(vec![Span::styled(
                    format!(
                        "  Profit per share: {}",
                        format_price_cents(1.0 - opp.price)
                    ),
                    Style::default().fg(Color::Green),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Note: Actual profit depends on available",
                    Style::default().fg(Color::DarkGray),
                )]),
                Line::from(vec![Span::styled(
                    "liquidity. Large orders cause price slippage.",
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

            let event_loading = Paragraph::new("").block(
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
                    "Yield Calculation:",
                    Style::default().fg(Color::Yellow).bold(),
                )]),
                Line::from(vec![Span::styled(
                    format!(
                        "  Buy at {} -> Get $1.00 if {} wins",
                        format_price_cents(opp.price),
                        opp.outcome
                    ),
                    Style::default().fg(Color::DarkGray),
                )]),
                Line::from(vec![Span::styled(
                    format!(
                        "  Profit per share: {}",
                        format_price_cents(1.0 - opp.price)
                    ),
                    Style::default().fg(Color::Green),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Note: Actual profit depends on available",
                    Style::default().fg(Color::DarkGray),
                )]),
                Line::from(vec![Span::styled(
                    "liquidity. Large orders cause price slippage.",
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

        let market_empty = Paragraph::new("").block(
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
                Cell::from(format!("{}", markets_count)).style(Style::default().fg(Color::Cyan)),
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
        // Look up event from cache
        if let Some(event) = app.get_cached_event(&result.event_slug) {
            // Calculate dynamic height for event info based on content
            let event_url = format!("https://polymarket.com/event/{}", event.slug);
            let available_width = area.width.saturating_sub(8) as usize;
            let url_lines = (event_url.len() / available_width.max(1)) + 1;
            let has_tags = !event.tags.is_empty();
            let event_panel_height = 8
                + url_lines as u16
                + if has_tags {
                    1
                } else {
                    0
                };

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
                        "Yield Calculation:",
                        Style::default().fg(Color::Yellow).bold(),
                    )]),
                    Line::from(vec![Span::styled(
                        format!(
                            "  Buy at {} -> Get $1.00 if {} wins",
                            format_price_cents(y.price),
                            y.outcome
                        ),
                        Style::default().fg(Color::DarkGray),
                    )]),
                    Line::from(vec![Span::styled(
                        format!("  Profit per share: {}", format_price_cents(1.0 - y.price)),
                        Style::default().fg(Color::Green),
                    )]),
                    Line::from(""),
                    Line::from(vec![Span::styled(
                        "Note: Actual profit depends on available",
                        Style::default().fg(Color::DarkGray),
                    )]),
                    Line::from(vec![Span::styled(
                        "liquidity. Large orders cause price slippage.",
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

            let event_loading = Paragraph::new("").block(
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

        let yield_empty = Paragraph::new("").block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Yield Details"),
        );
        f.render_widget(yield_empty, chunks[1]);
    }
}
