//! Render functions for the trending TUI

use {
    super::state::{EventFilter, FocusedPanel, SearchMode, TrendingAppState},
    chrono::{DateTime, Utc},
    polymarket_api::gamma::Event,
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        prelude::Stylize,
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{
            Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Scrollbar,
            ScrollbarOrientation, ScrollbarState, Table, Wrap,
        },
    },
    unicode_width::UnicodeWidthStr,
};

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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height), // Header (with search if active)
            Constraint::Min(0),                // Main content
            Constraint::Length(8),             // Logs area
            Constraint::Length(3),             // Footer
        ])
        .split(f.size());

    // Header
    let watched_count = app
        .trades
        .event_trades
        .values()
        .filter(|et| et.is_watching)
        .count();
    let filtered_count = app.filtered_events().len();

    if app.is_in_filter_mode() {
        // Split header into info and search input
        let header_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Info line
                Constraint::Length(4), // Search input (with borders - increased height)
            ])
            .split(chunks[0]);

        // Render filter options in header (even in search mode, show current filter)
        let is_header_focused = app.navigation.focused_panel == FocusedPanel::Header;
        let header_block_style = if is_header_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        // Build filter options line with selection highlighting
        let filter_options = vec![
            (EventFilter::Trending, "Trending"),
            (EventFilter::Breaking, "Breaking"),
            (EventFilter::New, "New"),
        ];

        let mut filter_spans = Vec::new();
        for (filter, label) in &filter_options {
            let is_selected = *filter == app.event_filter;
            let style = if is_selected {
                if is_header_focused {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                }
            } else {
                Style::default().fg(Color::Gray)
            };

            if !filter_spans.is_empty() {
                filter_spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            }
            filter_spans.push(Span::styled(*label, style));
        }

        let header_text = format!(
            "Showing {}/{} events | Watching: {} | Press Esc to exit search",
            filtered_count,
            app.events.len(),
            watched_count
        );
        let header = Paragraph::new(vec![
            Line::from("🔥 Polymarket".fg(Color::Yellow).bold()),
            Line::from(filter_spans),
            Line::from(header_text),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(if is_header_focused {
                    "Polymarket (Focused)"
                } else {
                    "Polymarket"
                })
                .border_style(header_block_style),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
        f.render_widget(header, header_chunks[0]);

        // Search input field - show full query with proper spacing
        let search_line = if app.search.query.is_empty() {
            let prompt_text = match app.search.mode {
                SearchMode::ApiSearch => "🔍 API Search: (type to search via API)",
                SearchMode::LocalFilter => "🔍 Filter: (type to filter current list)",
                SearchMode::None => "🔍 Search: (type to search)",
            };
            Line::from(prompt_text.fg(Color::DarkGray))
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
        let search_input = Paragraph::new(vec![search_line])
            .block(Block::default().borders(Borders::ALL).title("Search"))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });
        f.render_widget(search_input, header_chunks[1]);
    } else {
        // Render filter options in header
        let is_header_focused = app.navigation.focused_panel == FocusedPanel::Header;
        let header_block_style = if is_header_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        // Build filter options line with selection highlighting
        let filter_options = vec![
            (EventFilter::Trending, "Trending"),
            (EventFilter::Breaking, "Breaking"),
            (EventFilter::New, "New"),
        ];

        let mut filter_spans = Vec::new();
        for (filter, label) in &filter_options {
            let is_selected = *filter == app.event_filter;
            let style = if is_selected {
                if is_header_focused {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                }
            } else {
                Style::default().fg(Color::Gray)
            };

            if !filter_spans.is_empty() {
                filter_spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            }
            filter_spans.push(Span::styled(*label, style));
        }

        let header_text = format!(
            "Showing {} events | Watching: {} | Press '/' for API search, 'f' for local filter | Use ↑↓ to navigate | Enter to watch/unwatch | 'q' to quit",
            filtered_count, watched_count
        );
        let header = Paragraph::new(vec![
            Line::from("🔥 Polymarket".fg(Color::Yellow).bold()),
            Line::from(filter_spans),
            Line::from(header_text),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(if is_header_focused {
                    "Polymarket (Focused)"
                } else {
                    "Polymarket"
                })
                .border_style(header_block_style),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
        f.render_widget(header, chunks[0]);
    }

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
            let is_watching = app.is_watching(&event.slug);
            let trade_count = app.get_trades(&event.slug).len();

            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(Color::White)
            };

            let markets_count = event.markets.len();

            // Format: "title ...spaces... trades / markets" (right-aligned)
            // Account for List widget borders (2 chars) and some padding
            let usable_width = area.width.saturating_sub(2) as usize; // -2 for borders

            // Build the right-aligned text: "markets" or "trades / markets" if watching
            let right_text = if is_watching {
                format!("{} / {}", trade_count, markets_count)
            } else {
                markets_count.to_string()
            };
            let right_text_width = right_text.width();

            // Reserve space for right text + 1 space padding
            let reserved_width = right_text_width + 1;
            let available_width = usable_width.saturating_sub(reserved_width);

            // Truncate title to fit available space (using display width)
            let title = truncate_to_width(&event.title, available_width);

            let title_width = title.width();
            let remaining_width = usable_width
                .saturating_sub(title_width)
                .saturating_sub(right_text_width);

            let mut line_spans = vec![Span::styled(title, style)];

            // Add spaces to right-align the markets/trades count
            if remaining_width > 0 {
                line_spans.push(Span::styled(" ".repeat(remaining_width), Style::default()));
            }

            // Add the right-aligned text with appropriate styling
            if is_watching {
                // Show "trades / markets" with trades in green and markets in cyan
                line_spans.push(Span::styled(
                    trade_count.to_string(),
                    Style::default().fg(Color::Green),
                ));
                line_spans.push(Span::styled(" / ", Style::default().fg(Color::Gray)));
                line_spans.push(Span::styled(
                    markets_count.to_string(),
                    Style::default().fg(Color::Cyan),
                ));
            } else {
                // Just show markets count
                line_spans.push(Span::styled(
                    markets_count.to_string(),
                    Style::default().fg(Color::Cyan),
                ));
            }

            ListItem::new(Line::from(line_spans))
        })
        .collect();

    let is_focused = app.navigation.focused_panel == FocusedPanel::EventsList;
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(if is_focused {
                    "Trending Events (Focused)"
                } else {
                    "Trending Events"
                })
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
        let min_event_details_height = 8; // Minimum height (6 base lines + 2 for borders)

        // Split area into event details, markets, and trades
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
                .skip(scroll)
                .take(visible_height)
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
            .column_spacing(1);

            f.render_widget(table, chunks[2]);

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
    trade_count: usize,
    area: Rect,
) {
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
    lines.push(Line::from(vec![
        Span::styled("Total Volume: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(
            volume_str,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" | ", Style::default().fg(Color::Gray)),
        Span::styled("Trades: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(
            trade_count.to_string(),
            Style::default().fg(if is_watching {
                Color::Green
            } else {
                Color::Gray
            }),
        ),
    ]));

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
        .skip(scroll)
        .take(visible_height)
        .map(|market| {
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

            ListItem::new(Line::from(line_spans))
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
