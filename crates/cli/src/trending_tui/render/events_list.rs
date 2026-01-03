//! Events list rendering for the trending TUI

use {
    super::utils::{event_has_yield, format_volume, truncate_to_width},
    crate::trending_tui::state::{EventFilter, EventSortBy, FocusedPanel, TrendingAppState},
    ratatui::{
        Frame,
        layout::Rect,
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{
            Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Scrollbar,
            ScrollbarOrientation, ScrollbarState,
        },
    },
    unicode_width::UnicodeWidthStr,
};

pub fn render_events_list(f: &mut Frame, app: &TrendingAppState, area: Rect) {
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
            let (metric_str, metric_color) = if app.event_filter == EventFilter::Breaking {
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
