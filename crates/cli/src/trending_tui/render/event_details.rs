//! Event details panel rendering functions

use {
    super::utils::truncate,
    crate::trending_tui::state::{FocusedPanel, TrendingAppState},
    chrono::{DateTime, Utc},
    polymarket_api::gamma::Event,
    ratatui::{
        Frame,
        layout::Rect,
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{
            Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
            Wrap,
        },
    },
};

pub fn render_event_details(
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
