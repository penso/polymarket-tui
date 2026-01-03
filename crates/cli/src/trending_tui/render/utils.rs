//! Utility functions for rendering

use {
    ratatui::{layout::Rect, style::Color},
    unicode_width::UnicodeWidthStr,
};

/// Format a number with thousands separators (e.g., 1234567 -> "1,234,567")
pub fn format_with_thousands(n: f64, decimals: usize) -> String {
    let formatted = format!("{:.prec$}", n, prec = decimals);
    let parts: Vec<&str> = formatted.split('.').collect();
    let int_part = parts[0];

    // Add thousands separators to integer part
    let chars: Vec<char> = int_part.chars().collect();
    let mut result = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(*c);
    }

    if decimals > 0 && parts.len() > 1 {
        format!("{}.{}", result, parts[1])
    } else {
        result
    }
}

/// Format a price (0.0-1.0) as cents like the Polymarket website
/// Uses 1 decimal place for sub-cent and high prices to match website rounding
/// Examples: 0.01 -> "1¢", 0.11 -> "11¢", 0.89 -> "89¢", 0.003 -> "0.3¢", 0.998 -> "99.8¢"
pub fn format_price_cents(price: f64) -> String {
    let cents = price * 100.0;
    if cents < 0.1 {
        // Very small prices, show with 2 decimal places
        format!("{:.2}¢", cents)
    } else if cents < 1.0 {
        // Sub-cent prices, show with 1 decimal place (e.g., 0.3¢)
        format!("{:.1}¢", cents)
    } else if cents < 10.0 {
        format!("{:.1}¢", cents)
    } else if cents > 99.0 && cents < 100.0 {
        // High prices (99-100%), show with 1 decimal place to match website
        format!("{:.1}¢", cents)
    } else {
        format!("{:.0}¢", cents)
    }
}

/// Format a volume/liquidity value with appropriate units (K, M)
pub fn format_volume(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("${:.1}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("${:.0}K", value / 1_000.0)
    } else if value > 0.0 {
        format!("${:.0}", value)
    } else {
        String::new()
    }
}

/// Truncate a string to a maximum number of characters
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
pub fn format_pnl(value: f64) -> (String, Color) {
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
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
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

/// Yield opportunity threshold (95% probability = 5% potential return)
pub const YIELD_MIN_PROB: f64 = 0.95;

/// Check if a market has a yield opportunity (any outcome with price >= 95% and < 100%)
pub fn market_has_yield(market: &polymarket_api::gamma::Market) -> bool {
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
pub fn event_has_yield(event: &polymarket_api::gamma::Event) -> bool {
    event.markets.iter().any(market_has_yield)
}

/// Create a centered rectangle with percentage-based dimensions
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    use ratatui::layout::{Constraint, Direction, Layout};

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

/// Create a centered rectangle with fixed width and percentage height
pub fn centered_rect_fixed_width(width: u16, percent_y: u16, r: Rect) -> Rect {
    use ratatui::layout::{Constraint, Direction, Layout};

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    // Calculate horizontal margins
    let left_margin = (r.width.saturating_sub(width)) / 2;
    let right_margin = r.width.saturating_sub(width).saturating_sub(left_margin);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_margin),
            Constraint::Length(width),
            Constraint::Length(right_margin),
        ])
        .split(popup_layout[1])[1]
}
