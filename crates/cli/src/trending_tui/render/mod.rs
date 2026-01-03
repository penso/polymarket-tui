//! Render functions for the trending TUI

mod favorites;
mod header;
mod logs;
mod orderbook;
mod popups;
pub mod utils;
mod yield_tab;

use {
    favorites::render_favorites_tab,
    header::render_header,
    logs::render_logs,
    orderbook::render_orderbook,
    popups::render_popup,
    utils::{
        YIELD_MIN_PROB, event_has_yield, format_price_cents, format_volume, market_has_yield,
        truncate_to_width,
    },
    yield_tab::render_yield_tab,
};
pub use {orderbook::check_orderbook_title_click, utils::truncate};

use {
    super::state::{FocusedPanel, MainTab, SearchMode, TrendingAppState},
    chrono::{DateTime, Utc},
    polymarket_api::gamma::Event,
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{
            Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Scrollbar,
            ScrollbarOrientation, ScrollbarState, Table, Wrap,
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

            // Show metric based on current sort option (or price change for Breaking tab)
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
                    // Show metric based on current sort option
                    use crate::trending_tui::state::EventSortBy;
                    match app.event_sort_by {
                        EventSortBy::Volume24hr => {
                            // Calculate 24h volume from all markets
                            let total_volume: f64 = event
                                .markets
                                .iter()
                                .map(|m| m.volume_24hr.unwrap_or(0.0))
                                .sum();
                            (format_volume(total_volume), Color::Green)
                        },
                        EventSortBy::VolumeTotal => {
                            // Use event's total volume or sum from markets
                            let total_volume = event.volume.unwrap_or_else(|| {
                                event
                                    .markets
                                    .iter()
                                    .map(|m| m.volume_total.unwrap_or(0.0))
                                    .sum()
                            });
                            (format_volume(total_volume), Color::Green)
                        },
                        EventSortBy::Liquidity | EventSortBy::Newest | EventSortBy::EndingSoon => {
                            // Show liquidity for these sort options
                            let liquidity = event.liquidity.unwrap_or(0.0);
                            (format_volume(liquidity), Color::Cyan)
                        },
                        EventSortBy::Competitive => {
                            // Show competitive score as percentage
                            if let Some(competitive) = event.competitive {
                                (format!("{:.0}%", competitive * 100.0), Color::Magenta)
                            } else {
                                (String::new(), Color::Magenta)
                            }
                        },
                    }
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

    // Build title with sort option and search query if applicable (count moved to bottom)
    let event_count = app.filtered_events().len();
    let sort_label = app.event_sort_by.label();
    let title = if !app.search.last_searched_query.is_empty() && !app.search.results.is_empty() {
        // Show search query in title when displaying API search results
        format!(
            "Events - Sort: {} - \"{}\"",
            sort_label, app.search.last_searched_query
        )
    } else {
        format!("Events - Sort: {}", sort_label)
    };

    // Build position indicator for bottom right (lazygit style)
    let position_indicator = if event_count > 0 {
        format!("{} of {}", selected_index + 1, event_count)
    } else {
        "0 of 0".to_string()
    };

    // Build block with position indicator at bottom right
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(block_style);

    // Add status or position indicator at bottom (lazygit style: "1 of 50─" with trailing dash)
    if app.pagination.is_fetching_more {
        block = block.title_bottom(Line::from(vec![
            Span::raw(" Loading more... "),
            Span::raw(" ".repeat(10)), // spacer
            Span::raw(format!("{}─", position_indicator)),
        ]));
    } else if app.search.is_searching {
        block = block.title_bottom(Line::from(vec![
            Span::raw(" Searching... "),
            Span::raw(" ".repeat(10)), // spacer
            Span::raw(format!("{}─", position_indicator)),
        ]));
    } else {
        block = block.title_bottom(Line::from(format!("{}─", position_indicator)).right_aligned());
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
            f.render_widget(paragraph, chunks[3]);
        } else {
            // Calculate visible rows and apply scroll
            let visible_height = (chunks[3].height as usize).saturating_sub(3); // -3 for header
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
                f.render_stateful_widget(table, chunks[3], &mut table_state);
            } else {
                f.render_widget(table, chunks[3]);
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
                    chunks[3],
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

/// Render the trades panel for a given set of trades and watching status
pub(super) fn render_trades_panel(
    f: &mut Frame,
    app: &TrendingAppState,
    trades: &[crate::trending_tui::state::Trade],
    is_watching: bool,
    area: Rect,
) {
    let is_focused = app.navigation.focused_panel == FocusedPanel::Trades;
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

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
        f.render_widget(paragraph, area);
    } else {
        // Calculate visible rows and apply scroll
        let visible_height = (area.height as usize).saturating_sub(3);
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

                let title_truncated = truncate(&trade.title, 30);
                let user_display = if !trade.user.is_empty() {
                    &trade.user
                } else if !trade.pseudonym.is_empty() {
                    &trade.pseudonym
                } else {
                    "-"
                };
                let user_truncated = truncate(user_display, 15);

                let bg_color = if idx % 2 == 0 {
                    Color::Reset
                } else {
                    Color::Rgb(30, 30, 40)
                };

                Row::new(vec![
                    Cell::from(time).style(Style::default().fg(Color::Gray)),
                    Cell::from(trade.side.clone()).style(side_style),
                    Cell::from(trade.outcome.clone()).style(outcome_style),
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
            Constraint::Length(9),
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Fill(1),
            Constraint::Length(12),
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(if is_focused {
                    format!("Trades ({}) (Focused)", trades.len())
                } else {
                    format!("Trades ({})", trades.len())
                })
                .border_style(block_style),
        )
        .column_spacing(1);

        f.render_widget(table, area);

        // Render scrollbar if needed
        if total_rows > visible_height {
            let mut scrollbar_state = ScrollbarState::new(total_rows)
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
}

pub(super) fn render_event_details(
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

pub(super) fn render_markets(f: &mut Frame, app: &TrendingAppState, event: &Event, area: Rect) {
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

    // Fixed column widths for alignment - compact layout
    // Yield: "+XX.X%" = 6 chars max
    // Volume: "$XXX.XM" = 7 chars max
    // Buttons combined: "[XXXXXXXX XX.X¢][XXXXXXXX XX.X¢]" = 32 chars max (adjacent, no space)
    const YIELD_COL_WIDTH: usize = 6;
    const VOLUME_COL_WIDTH: usize = 7;
    const BUTTONS_COL_WIDTH: usize = 32; // Both buttons combined

    // Calculate total fixed right content width for active markets
    // Layout: [yield 6][space][volume 7][space][buttons 32] = 46
    let fixed_right_width = YIELD_COL_WIDTH + 1 + VOLUME_COL_WIDTH + 1 + BUTTONS_COL_WIDTH;
    let usable_width = (area.width as usize).saturating_sub(2); // -2 for borders
    let icon_width = 2; // "● " or "$ " etc.

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

            // Check if this market is selected for orderbook display
            let is_orderbook_selected = idx == app.orderbook_state.selected_market_index;

            // Status indicator: ● for active, ◐ for in-review, ○ for resolved, $ for yield
            let status_icon = if market.closed {
                "○ "
            } else if has_yield {
                "$ " // Yield opportunity indicator
            } else if market.is_in_review() {
                "◐ "
            } else {
                "● "
            };

            // Build outcome display string for closed markets
            let outcomes_str = if market.closed {
                // For resolved markets, show only the winning side
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
                String::new()
            };

            // Get prices for active markets (for Buy buttons)
            // Priority: 1) orderbook best ask (for selected market), 2) market_prices from batch API, 3) outcome_prices
            let (yes_price, no_price): (Option<f64>, Option<f64>) = if !market.closed {
                // Check if this is the selected market with orderbook data
                let orderbook_price = if is_orderbook_selected {
                    app.orderbook_state
                        .orderbook
                        .as_ref()
                        .and_then(|ob| ob.asks.first().map(|level| level.price))
                } else {
                    None
                };

                // For the selected market, use orderbook price based on which outcome is displayed
                let (yes_from_orderbook, no_from_orderbook) = if is_orderbook_selected {
                    match app.orderbook_state.selected_outcome {
                        crate::trending_tui::state::OrderbookOutcome::Yes => {
                            (orderbook_price, None)
                        },
                        crate::trending_tui::state::OrderbookOutcome::No => (None, orderbook_price),
                    }
                } else {
                    (None, None)
                };

                let yes = yes_from_orderbook.or_else(|| {
                    if let Some(ref token_ids) = market.clob_token_ids {
                        token_ids
                            .first()
                            .and_then(|asset_id| app.market_prices.get(asset_id).copied())
                            .or_else(|| {
                                market
                                    .outcome_prices
                                    .first()
                                    .and_then(|p| p.parse::<f64>().ok())
                            })
                    } else {
                        market
                            .outcome_prices
                            .first()
                            .and_then(|p| p.parse::<f64>().ok())
                    }
                });
                let no = no_from_orderbook.or_else(|| {
                    if let Some(ref token_ids) = market.clob_token_ids {
                        token_ids
                            .get(1)
                            .and_then(|asset_id| app.market_prices.get(asset_id).copied())
                            .or_else(|| {
                                market
                                    .outcome_prices
                                    .get(1)
                                    .and_then(|p| p.parse::<f64>().ok())
                            })
                    } else {
                        market
                            .outcome_prices
                            .get(1)
                            .and_then(|p| p.parse::<f64>().ok())
                    }
                });
                (yes, no)
            } else {
                (None, None)
            };

            // Build Buy buttons for active markets using actual outcome names
            let (yes_button, no_button) = if !market.closed {
                let yes_price_str = yes_price
                    .map(format_price_cents)
                    .unwrap_or_else(|| "N/A".to_string());
                let no_price_str = no_price
                    .map(format_price_cents)
                    .unwrap_or_else(|| "N/A".to_string());

                // Get outcome names, truncate to max 8 chars to keep buttons reasonable
                let outcome_0 = market
                    .outcomes
                    .first()
                    .map(|s| truncate(s, 8))
                    .unwrap_or_else(|| "Yes".to_string());
                let outcome_1 = market
                    .outcomes
                    .get(1)
                    .map(|s| truncate(s, 8))
                    .unwrap_or_else(|| "No".to_string());

                (
                    format!("[{} {}]", outcome_0, yes_price_str),
                    format!("[{} {}]", outcome_1, no_price_str),
                )
            } else {
                (String::new(), String::new())
            };

            // Format yield return string if applicable
            let yield_str = yield_return.map(|ret| format!("+{:.1}%", ret));

            let has_buttons = !market.closed;

            // Calculate available width for question
            let right_content_width = if has_buttons {
                fixed_right_width
            } else {
                // For closed markets: just outcomes + volume
                let outcomes_width = outcomes_str.width();
                let vol_width = volume_str.len();
                outcomes_width + 1 + vol_width
            };
            let available_width = usable_width
                .saturating_sub(right_content_width)
                .saturating_sub(icon_width)
                .saturating_sub(1); // 1 space padding

            // Truncate question to fit available width
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

            // Start with status icon - use original colors
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

            if has_buttons {
                // For active markets: compact layout with buttons right-aligned to panel edge
                // Yield column (right-aligned within YIELD_COL_WIDTH)
                let yield_display = yield_str.as_deref().unwrap_or("");
                let yield_padded = format!("{:>width$}", yield_display, width = YIELD_COL_WIDTH);
                line_spans.push(Span::styled(
                    yield_padded,
                    Style::default().fg(Color::Yellow),
                ));
                line_spans.push(Span::styled(" ", Style::default()));

                // Volume column (right-aligned within VOLUME_COL_WIDTH)
                let volume_padded = format!("{:>width$}", volume_str, width = VOLUME_COL_WIDTH);
                line_spans.push(Span::styled(
                    volume_padded,
                    Style::default().fg(Color::Green),
                ));
                line_spans.push(Span::styled(" ", Style::default()));

                // Both buttons combined and right-aligned as a single unit (no space between)
                let buttons_combined = format!("{}{}", yes_button, no_button);
                let buttons_width = buttons_combined.len();
                // Add padding before buttons to right-align them
                let buttons_padding = BUTTONS_COL_WIDTH.saturating_sub(buttons_width);
                if buttons_padding > 0 {
                    line_spans.push(Span::raw(" ".repeat(buttons_padding)));
                }
                line_spans.push(Span::styled(yes_button, Style::default().fg(Color::Green)));
                line_spans.push(Span::styled(no_button, Style::default().fg(Color::Red)));
            } else {
                // For closed markets: show outcomes and volume
                if !outcomes_str.is_empty() {
                    line_spans.push(Span::styled(
                        outcomes_str.clone(),
                        Style::default().fg(Color::Cyan),
                    ));
                    if !volume_str.is_empty() {
                        line_spans.push(Span::styled(" ", Style::default()));
                    }
                }
                if !volume_str.is_empty() {
                    line_spans.push(Span::styled(
                        volume_str.clone(),
                        Style::default().fg(Color::Green),
                    ));
                }
            }

            // Background color: highlight selected market, otherwise zebra striping
            let bg_color = if is_orderbook_selected {
                Color::Rgb(60, 60, 80) // Highlight selected market (same as events list)
            } else if idx % 2 == 0 {
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

    // Build title (without count, moved to bottom)
    let title = if is_focused {
        "Markets (Focused)"
    } else {
        "Markets"
    };

    // Build position indicator for bottom right (lazygit style)
    let selected_idx = app.orderbook_state.selected_market_index;
    let position_indicator = if total_markets > 0 {
        format!("{} of {}", selected_idx + 1, total_markets)
    } else {
        "0 of 0".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .title_bottom(Line::from(format!("{}─", position_indicator)).right_aligned())
        .border_style(block_style);

    let list = List::new(items).block(block);

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
