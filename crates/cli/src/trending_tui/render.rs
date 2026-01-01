//! Render functions for the trending TUI

use {
    super::state::{EventFilter, FocusedPanel, PopupType, SearchMode, TrendingAppState},
    chrono::{DateTime, Utc},
    polymarket_api::gamma::Event,
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Direction, Layout, Rect, Spacing},
        prelude::Stylize,
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{
            Block, Borders, Cell, Clear, LineGauge, List, ListItem, ListState, Paragraph, Row,
            Scrollbar, ScrollbarOrientation, ScrollbarState, Table, Tabs, Wrap,
        },
    },
    unicode_width::UnicodeWidthStr,
};

/// Check if a click is on a tab and return which filter it corresponds to
/// Tabs are rendered on the first line of the header area
/// Tab format with default padding: " Trending  Breaking  New "
pub fn get_clicked_tab(x: u16, y: u16, _size: Rect) -> Option<EventFilter> {
    // Tabs are on the first line (y = 0)
    if y != 0 {
        return None;
    }

    // Tabs widget with default padding (" ") and divider (" "):
    // Format: " Trending  Breaking  New "
    // Positions: 0-9 = " Trending", 10-19 = " Breaking", 20-24 = " New"
    // Each tab has 1 space padding on each side, divider is 1 space
    let tab_ranges = [
        (0u16, 10u16, EventFilter::Trending),  // " Trending "
        (10u16, 20u16, EventFilter::Breaking), // " Breaking "
        (20u16, 25u16, EventFilter::New),      // " New "
    ];

    for (start, end, filter) in tab_ranges {
        if x >= start && x < end {
            return Some(filter);
        }
    }
    None
}

/// Format a price (0.0-1.0) as cents like the Polymarket website
/// Examples: 0.01 -> "1¢", 0.11 -> "11¢", 0.89 -> "89¢", 0.004 -> "<1¢"
fn format_price_cents(price: f64) -> String {
    let cents = price * 100.0;
    if cents < 1.0 {
        "<1¢".to_string()
    } else if cents < 10.0 {
        format!("{:.1}¢", cents)
    } else {
        format!("{:.0}¢", cents)
    }
}

pub fn render(f: &mut Frame, app: &mut TrendingAppState) {
    // Header height: 3 lines for normal mode (title, filters, info), 6 for search mode
    let header_height = if app.is_in_filter_mode() {
        6
    } else {
        4
    };
    // Use Spacing::Overlap(1) to collapse borders between vertical sections
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .spacing(Spacing::Overlap(1))
        .constraints([
            Constraint::Length(header_height), // Header (with search if active)
            Constraint::Min(0),                // Main content
            Constraint::Length(8),             // Logs area
            Constraint::Length(3),             // Footer
        ])
        .split(f.area());

    // Header
    let watched_count = app
        .trades
        .event_trades
        .values()
        .filter(|et| et.is_watching)
        .count();
    let filtered_count = app.filtered_events().len();

    // Helper to get selected tab index
    let selected_tab_index = match app.event_filter {
        EventFilter::Trending => 0,
        EventFilter::Breaking => 1,
        EventFilter::New => 2,
    };

    if app.is_in_filter_mode() {
        // Split header into tabs, info, and search input
        let header_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tabs line
                Constraint::Length(2), // Info line
                Constraint::Length(3), // Search input
            ])
            .split(chunks[0]);

        let is_header_focused = app.navigation.focused_panel == FocusedPanel::Header;

        // Render tabs using Tabs widget
        let tab_titles = vec!["Trending", "Breaking", "New"];
        let tabs = Tabs::new(tab_titles)
            .select(selected_tab_index)
            .style(Style::default().fg(Color::DarkGray))
            .highlight_style(
                Style::default()
                    .fg(if is_header_focused {
                        Color::Yellow
                    } else {
                        Color::Cyan
                    })
                    .add_modifier(Modifier::BOLD),
            )
            .divider(" ");
        f.render_widget(tabs, header_chunks[0]);

        // Info line
        let header_text = format!(
            "🔥 {} events | Watching: {} | Esc to exit",
            filtered_count, watched_count
        );
        let info = Paragraph::new(header_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Left);
        f.render_widget(info, header_chunks[1]);

        // Search input field - show full query with proper spacing
        let search_line = if app.search.query.is_empty() {
            let prompt_text = match app.search.mode {
                SearchMode::ApiSearch => "🔍 API Search: (type to search via API)",
                SearchMode::LocalFilter => "🔍 Filter: (type to filter current list)",
                SearchMode::None => "🔍 Search: (type to search)",
            };
            Line::from(prompt_text.fg(Color::DarkGray))
        } else if app.search.is_searching {
            Line::from(vec![
                Span::styled("🔍 Search: ", Style::default().fg(Color::White)),
                Span::styled(
                    app.search.query.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ⏳", Style::default().fg(Color::Yellow)),
            ])
        } else {
            Line::from(vec![
                Span::styled("🔍 Search: ", Style::default().fg(Color::White)),
                Span::styled(
                    app.search.query.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        };
        let search_title = if app.search.is_searching {
            "Search ⏳ Searching..."
        } else {
            "Search"
        };
        let search_input = Paragraph::new(vec![search_line])
            .block(Block::default().borders(Borders::ALL).title(search_title))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });
        f.render_widget(search_input, header_chunks[2]);
    } else {
        // Normal mode: Split header into tabs and info
        let header_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tabs line
                Constraint::Length(3), // Info with border
            ])
            .split(chunks[0]);

        let is_header_focused = app.navigation.focused_panel == FocusedPanel::Header;

        // Render tabs using Tabs widget
        let tab_titles = vec!["Trending", "Breaking", "New"];
        let tabs = Tabs::new(tab_titles)
            .select(selected_tab_index)
            .style(Style::default().fg(Color::DarkGray))
            .highlight_style(
                Style::default()
                    .fg(if is_header_focused {
                        Color::Yellow
                    } else {
                        Color::Cyan
                    })
                    .add_modifier(Modifier::BOLD),
            )
            .divider(" ");
        f.render_widget(tabs, header_chunks[0]);

        // Info block
        let header_text = format!(
            "🔥 {} events | Watching: {} | /: Search | f: Filter | ←→: Tabs | q: Quit",
            filtered_count, watched_count
        );
        let info = Paragraph::new(header_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if is_header_focused {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    }),
            )
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Left);
        f.render_widget(info, header_chunks[1]);
    }

    // Main content - split into events list and trades view
    // Use Spacing::Overlap(1) to collapse borders between panels
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(Spacing::Overlap(1))
        .constraints([
            Constraint::Percentage(40), // Events list
            Constraint::Percentage(60), // Trades view
        ])
        .split(chunks[1]);

    render_events_list(f, app, main_chunks[0]);
    render_trades(f, app, main_chunks[1]);

    // Logs area
    render_logs(f, app, chunks[2]);

    // Footer - show focused panel info with context-sensitive help
    let panel_name = app.navigation.focused_panel.name();
    let panel_help = app.navigation.focused_panel.help_text();
    let footer_text = if app.search.mode == SearchMode::ApiSearch {
        format!("Type to search | Esc: Cancel | [{}]", panel_name)
    } else if app.search.mode == SearchMode::LocalFilter {
        format!("Type to filter | Esc: Cancel | [{}]", panel_name)
    } else {
        format!("{} | Tab: Switch | q: Quit | [{}]", panel_help, panel_name)
    };
    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));
    f.render_widget(footer, chunks[3]);

    // Render popup if active (on top of everything)
    if let Some(ref popup) = app.popup {
        render_popup(f, popup);
    }

    // Render loading gauge if actively loading
    if app.pagination.is_fetching_more || app.search.is_searching {
        render_loading_gauge(f, app);
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
fn render_popup(f: &mut Frame, popup: &PopupType) {
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
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Render a loading gauge at the bottom of the screen
fn render_loading_gauge(f: &mut Frame, app: &TrendingAppState) {
    // Position at the very bottom, above the footer
    let area = Rect {
        x: 1,
        y: f.area().height.saturating_sub(4),
        width: f.area().width.saturating_sub(2),
        height: 1,
    };

    let label = if app.search.is_searching {
        "Searching..."
    } else {
        "Loading more events..."
    };

    // Use an indeterminate progress (pulse effect via loading_progress)
    let gauge = LineGauge::default()
        .filled_style(Style::default().fg(Color::Cyan))
        .unfilled_style(Style::default().fg(Color::DarkGray))
        .filled_symbol("━")
        .unfilled_symbol("━")
        .ratio(app.loading_progress)
        .label(label);

    f.render_widget(gauge, area);
}

fn render_events_list(f: &mut Frame, app: &TrendingAppState, area: Rect) {
    let filtered_events = app.filtered_events();
    let items: Vec<ListItem> = filtered_events
        .iter()
        .enumerate()
        .skip(app.scroll.events_list)
        .take(area.height as usize - 2)
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
                format!("{}{}", trade_count_str, markets_count)
            } else {
                format!("{}{} {}", trade_count_str, volume_str, markets_count)
            };
            let right_text_width = right_text.width();

            // Reserve space for right text + 1 space padding + closed icon if needed
            let closed_icon = if is_closed {
                "✕ "
            } else {
                ""
            };
            let closed_icon_width = closed_icon.width();
            let reserved_width = right_text_width + 1 + closed_icon_width;
            let available_width = usable_width.saturating_sub(reserved_width);

            // Truncate title to fit available space (using display width)
            let title = truncate_to_width(&event.title, available_width);

            let title_width = title.width();
            let remaining_width = usable_width
                .saturating_sub(closed_icon_width)
                .saturating_sub(title_width)
                .saturating_sub(right_text_width);

            let mut line_spans = Vec::new();
            if is_closed {
                line_spans.push(Span::styled(closed_icon, Style::default().fg(Color::Red)));
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
            line_spans.push(Span::styled(
                markets_count.to_string(),
                Style::default().fg(Color::Cyan),
            ));

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

    // Build title with loading indicator
    let title = if app.search.is_searching {
        "Trending Events ⏳ Searching..."
    } else if app.pagination.is_fetching_more {
        "Trending Events ⏳ Loading..."
    } else if is_focused {
        "Trending Events (Focused)"
    } else {
        "Trending Events"
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(block_style),
        )
        .highlight_style(
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
        let min_event_details_height = 10; // Minimum height (8 base lines + 2 for borders)

        // Split area into event details, markets, and trades
        // Use Spacing::Overlap(1) to collapse borders between panels
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .spacing(Spacing::Overlap(1))
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
                    let user_truncated = truncate(&trade.user, 15);
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
                Constraint::Length(10),     // Time
                Constraint::Length(8),      // Side
                Constraint::Length(5),      // Outcome
                Constraint::Length(10),     // Price
                Constraint::Length(10),     // Shares
                Constraint::Length(10),     // Value
                Constraint::Percentage(30), // Title
                Constraint::Length(15),     // User
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

    // Build compact lines without blank lines
    let mut lines = vec![Line::from(vec![
        Span::styled("Title: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(
            truncate(&event.title, 60),
            Style::default().fg(Color::White),
        ),
    ])];

    lines.push(Line::from(vec![
        Span::styled("Slug: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(truncate(&event.slug, 60), Style::default().fg(Color::Blue)),
    ]));
    let event_url = format!("https://polymarket.com/event/{}", event.slug);
    lines.push(Line::from(vec![
        Span::styled("URL: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(event_url, Style::default().fg(Color::Cyan)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Event ID: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(truncate(&event.id, 50), Style::default().fg(Color::White)),
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
        if tags_text.len() <= available_width {
            lines.push(Line::from(vec![
                Span::styled("Tags: ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(tags_text, Style::default().fg(Color::Cyan)),
            ]));
        } else {
            // Tags need to wrap - split into multiple lines
            let tags_prefix = "Tags: ";
            let tags_content = &tags_text;
            let content_width = available_width.saturating_sub(tags_prefix.len());

            // First line with prefix
            let first_line_content = if tags_content.len() > content_width {
                truncate(tags_content, content_width)
            } else {
                tags_content.clone()
            };
            lines.push(Line::from(vec![
                Span::styled(tags_prefix, Style::default().fg(Color::Yellow).bold()),
                Span::styled(first_line_content, Style::default().fg(Color::Cyan)),
            ]));

            // Additional wrapped lines (without prefix, indented)
            let remaining_content = if tags_content.len() > content_width {
                &tags_content[content_width..]
            } else {
                ""
            };

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

    let paragraph = Paragraph::new(visible_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(if is_focused {
                    "Event Details (Focused)"
                } else {
                    "Event Details"
                })
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
            .block(Block::default().borders(Borders::ALL).title("Markets"))
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
            let status_icon = if market.closed {
                "○ "
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

pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
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
