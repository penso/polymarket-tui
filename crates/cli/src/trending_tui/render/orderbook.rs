//! Orderbook panel rendering functions

use {
    super::utils::{format_with_thousands, truncate},
    crate::trending_tui::state::{FocusedPanel, OrderbookOutcome, TrendingAppState},
    polymarket_api::gamma::Event,
    ratatui::{
        Frame,
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Paragraph},
    },
};

/// Check if a click on the orderbook panel title should toggle the outcome
/// Returns Some(OrderbookOutcome) if a tab was clicked, None otherwise
/// The title format is: "{name0} - {name1}" starting at area.x + 1 (after border)
pub fn check_orderbook_title_click(
    click_x: u16,
    click_y: u16,
    orderbook_area: Rect,
    outcome_0_name: &str,
    outcome_1_name: &str,
) -> Option<OrderbookOutcome> {
    // Check if click is on the title row (first row of the panel, which is the border with title)
    if click_y != orderbook_area.y {
        return None;
    }

    // Title starts after the border character
    // Format: "╭Yes - No───..."
    // Position: border(1) then first outcome name
    let title_start_x = orderbook_area.x + 1; // After left border
    let name_0_start = title_start_x;
    let name_0_len = outcome_0_name.chars().count().min(8) as u16; // truncated to 8
    let name_0_end = name_0_start + name_0_len;
    let separator_len = 3u16; // " - "
    let name_1_start = name_0_end + separator_len;
    let name_1_len = outcome_1_name.chars().count().min(8) as u16;
    let name_1_end = name_1_start + name_1_len;

    if click_x >= name_0_start && click_x < name_0_end {
        Some(OrderbookOutcome::Yes)
    } else if click_x >= name_1_start && click_x < name_1_end {
        Some(OrderbookOutcome::No)
    } else {
        None
    }
}

/// Render the order book panel for the selected market
pub fn render_orderbook(f: &mut Frame, app: &TrendingAppState, event: &Event, area: Rect) {
    let orderbook_state = &app.orderbook_state;
    let selected_outcome = orderbook_state.selected_outcome;

    // Get the selected market from sorted list (non-closed first, same as render_markets)
    let mut sorted_markets: Vec<_> = event.markets.iter().collect();
    sorted_markets.sort_by_key(|m| m.closed);
    let selected_market_idx = orderbook_state
        .selected_market_index
        .min(sorted_markets.len().saturating_sub(1));
    let market = sorted_markets.get(selected_market_idx).copied();

    // Get outcome names from market (default to Yes/No if not available)
    let (outcome_0_name, outcome_1_name) = if let Some(m) = market {
        let name_0 = m
            .outcomes
            .first()
            .map(|s| truncate(s, 20))
            .unwrap_or_else(|| "Yes".to_string());
        let name_1 = m
            .outcomes
            .get(1)
            .map(|s| truncate(s, 20))
            .unwrap_or_else(|| "No".to_string());
        (name_0, name_1)
    } else {
        ("Yes".to_string(), "No".to_string())
    };

    // Build title with clickable tabs like lazygit: "Yes - No"
    // The selected outcome is highlighted, unselected is dimmed
    // Use shorter names to fit in the narrow depth chart panel (25% width)
    let truncated_name_0 = truncate(&outcome_0_name, 8);
    let truncated_name_1 = truncate(&outcome_1_name, 8);

    let title_line = Line::from(vec![
        if selected_outcome == OrderbookOutcome::Yes {
            Span::styled(
                truncated_name_0.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                truncated_name_0.clone(),
                Style::default().fg(Color::DarkGray),
            )
        },
        Span::styled(" - ", Style::default().fg(Color::DarkGray)),
        if selected_outcome == OrderbookOutcome::No {
            Span::styled(
                truncated_name_1.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                truncated_name_1.clone(),
                Style::default().fg(Color::DarkGray),
            )
        },
    ]);

    let is_focused = app.navigation.focused_panel == FocusedPanel::Markets; // TODO: Add FocusedPanel::Orderbook
    let block_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    // Check if we have orderbook data with actual orders
    let has_orders = orderbook_state
        .orderbook
        .as_ref()
        .map(|ob| !ob.bids.is_empty() || !ob.asks.is_empty())
        .unwrap_or(false);

    if has_orders {
        let orderbook = orderbook_state.orderbook.as_ref().unwrap();

        // Find max cumulative total for scaling the depth bars
        // Scale each side (bids/asks) independently for better visualization
        let max_bid_total = orderbook.bids.last().map(|l| l.total).unwrap_or(0.0);
        let max_ask_total = orderbook.asks.last().map(|l| l.total).unwrap_or(0.0);

        // Split area into two columns: depth chart (left) and price levels (right)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25), // Depth chart
                Constraint::Percentage(75), // Price levels
            ])
            .split(area);

        // Render depth chart (left side)
        let depth_block = Block::default()
            .borders(Borders::LEFT | Borders::TOP | Borders::BOTTOM)
            .border_type(BorderType::Rounded)
            .title(title_line.clone())
            .border_style(block_style);

        // Calculate row counts based on available data (up to 6 per side like website)
        // Panel height is now dynamic, so we show all available data up to the limit
        const MAX_PER_SIDE: usize = 6;
        let asks_count = orderbook.asks.len().min(MAX_PER_SIDE);
        let bids_count = orderbook.bids.len().min(MAX_PER_SIDE);

        // Depth visualization using bars scaled to max cumulative total
        let bar_max_width = (chunks[0].width as usize).saturating_sub(2);
        let mut depth_lines: Vec<Line> = Vec::new();

        // Add empty line to align with the header row in the price panel
        depth_lines.push(Line::from(vec![Span::raw("")]));

        // Show asks (sell orders) in red at the top - bars grow from right to left
        // Reversed so highest price (deepest) is at top, best ask at bottom
        // Scale asks relative to max_ask_total for proper visualization
        let asks_to_show: Vec<_> = orderbook.asks.iter().take(asks_count).collect();
        for level in asks_to_show.iter().rev() {
            let bar_width = if max_ask_total > 0.0 {
                ((level.total / max_ask_total) * bar_max_width as f64).max(1.0) as usize
            } else {
                1
            };
            let bar = "█".repeat(bar_width.min(bar_max_width));
            depth_lines.push(Line::from(vec![Span::styled(
                bar,
                Style::default().fg(Color::LightRed),
            )]));
        }

        // Add empty line for spread separator (spread is shown in the price panel)
        depth_lines.push(Line::from(vec![Span::raw("")]));

        // Show bids (buy orders) in green at the bottom
        // Best bid at top, lowest bid at bottom
        // Scale bids relative to max_bid_total for proper visualization
        for level in orderbook.bids.iter().take(bids_count) {
            let bar_width = if max_bid_total > 0.0 {
                ((level.total / max_bid_total) * bar_max_width as f64).max(1.0) as usize
            } else {
                1
            };
            let bar = "█".repeat(bar_width.min(bar_max_width));
            depth_lines.push(Line::from(vec![Span::styled(
                bar,
                Style::default().fg(Color::LightGreen),
            )]));
        }

        let depth_para = Paragraph::new(depth_lines).block(depth_block);
        f.render_widget(depth_para, chunks[0]);

        // Render price levels (right side)
        let levels_block = Block::default()
            .borders(Borders::RIGHT | Borders::TOP | Borders::BOTTOM)
            .border_type(BorderType::Rounded)
            .border_style(block_style);

        let panel_width = (chunks[1].width as usize).saturating_sub(2); // Account for border

        // Fixed column widths for alignment
        let price_width = 8;
        let shares_width = 12;
        let total_width = 14;
        let columns_width = price_width + shares_width + total_width;

        // Calculate left padding to right-align all columns within the panel
        let left_padding = panel_width.saturating_sub(columns_width);

        let mut level_lines: Vec<Line> = Vec::new();

        // Header - right aligned
        let header = format!(
            "{:padding$}{:>price$}{:>shares$}{:>total$}",
            "",
            "PRICE",
            "SHARES",
            "TOTAL",
            padding = left_padding,
            price = price_width,
            shares = shares_width,
            total = total_width
        );
        level_lines.push(Line::from(vec![Span::styled(
            header,
            Style::default().fg(Color::DarkGray).bold(),
        )]));

        // Helper to format price in cents or dollars (1 decimal place for cents)
        let format_price = |price: f64| -> String {
            let cents = price * 100.0;
            if cents >= 100.0 {
                format!("${:.2}", price)
            } else {
                format!("{:.1}¢", cents)
            }
        };

        // Helper to format a level line with proper alignment
        let format_level =
            |level: &crate::trending_tui::state::OrderbookLevel, price_color: Color| -> Line {
                let price_str = format_price(level.price);
                let shares_str = format_with_thousands(level.size, 0);
                let total_str = format!("${}", format_with_thousands(level.total, 2));

                let padding_span = Span::raw(" ".repeat(left_padding));
                let price_span = Span::styled(
                    format!("{:>width$}", price_str, width = price_width),
                    Style::default().fg(price_color),
                );
                let shares_span = Span::styled(
                    format!("{:>width$}", shares_str, width = shares_width),
                    Style::default().fg(Color::White),
                );
                let total_span = Span::styled(
                    format!("{:>width$}", total_str, width = total_width),
                    Style::default().fg(Color::White),
                );

                Line::from(vec![padding_span, price_span, shares_span, total_span])
            };

        // Asks (sell orders) - show in descending price order (same count as depth chart)
        for level in orderbook.asks.iter().take(asks_count).rev() {
            level_lines.push(format_level(level, Color::LightRed));
        }

        // Spread separator - right aligned
        if let Some(spread) = orderbook.spread {
            let spread_cents = spread * 100.0;
            // Always use 1 decimal place for consistency
            let spread_str = format!("─── Spread: {:.1}¢ ───", spread_cents);
            // Center the spread line
            let spread_padding = panel_width.saturating_sub(spread_str.chars().count()) / 2;
            level_lines.push(Line::from(vec![Span::styled(
                format!(
                    "{:>width$}",
                    spread_str,
                    width = spread_padding + spread_str.len()
                ),
                Style::default().fg(Color::Yellow),
            )]));
        }

        // Bids (buy orders) - same count as depth chart
        for level in orderbook.bids.iter().take(bids_count) {
            level_lines.push(format_level(level, Color::LightGreen));
        }

        let levels_para = Paragraph::new(level_lines).block(levels_block);
        f.render_widget(levels_para, chunks[1]);
    } else {
        // No orderbook data or empty orderbook - show appropriate message
        let market_is_closed = market.map(|m| m.closed).unwrap_or(false);
        let message = if market_is_closed {
            "Market is closed"
        } else if orderbook_state.is_loading {
            "Loading orderbook..."
        } else if orderbook_state.orderbook.is_some() {
            // We have an orderbook but it's empty (no orders)
            "No orders in orderbook"
        } else if market.is_some() {
            "Loading orderbook..."
        } else {
            "No markets available"
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(title_line)
            .border_style(block_style);

        let paragraph = Paragraph::new(message)
            .block(block)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, area);
    }
}
