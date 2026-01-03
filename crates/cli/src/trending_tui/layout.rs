//! Layout calculations for panel areas

use {
    super::state::{FocusedPanel, MainTab},
    ratatui::layout::{Constraint, Direction, Layout, Rect},
};

/// Helper to calculate panel areas for mouse click detection
/// Returns (header_area, events_list_area, event_details_area, markets_area, trades_area, logs_area)
pub fn calculate_panel_areas(
    size: Rect,
    is_in_filter_mode: bool,
    show_logs: bool,
    main_tab: MainTab,
) -> (Rect, Rect, Rect, Rect, Rect, Rect) {
    let header_height = if is_in_filter_mode {
        5
    } else {
        2
    };
    // No overlap - all panels have full borders
    // Conditionally include logs area
    let constraints: Vec<Constraint> = if show_logs {
        vec![
            Constraint::Length(header_height),
            Constraint::Min(0),
            Constraint::Length(8),
            Constraint::Length(3),
        ]
    } else {
        vec![
            Constraint::Length(header_height),
            Constraint::Min(0),
            Constraint::Length(3),
        ]
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    let header_area = chunks[0];
    let logs_area = if show_logs {
        chunks[2]
    } else {
        Rect::default() // Empty rect when logs hidden
    };

    // For Yield tab, layout is: 55% list on left, details on right (Event + Market Details)
    if main_tab == MainTab::Yield {
        let yield_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Fill(1)])
            .split(chunks[1]);

        let yield_list_area = yield_chunks[0];

        // Right side: Event info (10 lines) + Market Details (rest)
        let yield_details_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(10), Constraint::Min(0)])
            .split(yield_chunks[1]);

        return (
            header_area,
            yield_list_area,         // Yield list (maps to EventsList)
            yield_details_chunks[0], // Event info (maps to EventDetails)
            yield_details_chunks[1], // Market details (maps to Markets)
            Rect::default(),         // No trades panel in Yield tab
            logs_area,
        );
    }

    // Trending tab: Main content split - no overlap for full borders
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Fill(1)])
        .split(chunks[1]);

    let events_list_area = main_chunks[0];

    // Right side split (event details, markets, trades) - no overlap for full borders
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Min(0),
        ])
        .split(main_chunks[1]);

    let event_details_area = right_chunks[0];
    let markets_area = right_chunks[1];
    let trades_area = right_chunks[2];

    (
        header_area,
        events_list_area,
        event_details_area,
        markets_area,
        trades_area,
        logs_area,
    )
}

/// Determine which panel was clicked based on coordinates
pub fn get_panel_at_position(
    x: u16,
    y: u16,
    size: Rect,
    is_in_filter_mode: bool,
    show_logs: bool,
    main_tab: MainTab,
) -> Option<FocusedPanel> {
    let (header, events_list, event_details, markets, trades, logs) =
        calculate_panel_areas(size, is_in_filter_mode, show_logs, main_tab);

    if y >= header.y && y < header.y + header.height && x >= header.x && x < header.x + header.width
    {
        Some(FocusedPanel::Header)
    } else if y >= events_list.y
        && y < events_list.y + events_list.height
        && x >= events_list.x
        && x < events_list.x + events_list.width
    {
        Some(FocusedPanel::EventsList)
    } else if y >= event_details.y
        && y < event_details.y + event_details.height
        && x >= event_details.x
        && x < event_details.x + event_details.width
    {
        Some(FocusedPanel::EventDetails)
    } else if y >= markets.y
        && y < markets.y + markets.height
        && x >= markets.x
        && x < markets.x + markets.width
    {
        Some(FocusedPanel::Markets)
    } else if y >= trades.y
        && y < trades.y + trades.height
        && x >= trades.x
        && x < trades.x + trades.width
    {
        Some(FocusedPanel::Trades)
    } else if show_logs
        && y >= logs.y
        && y < logs.y + logs.height
        && x >= logs.x
        && x < logs.x + logs.width
    {
        Some(FocusedPanel::Logs)
    } else {
        None
    }
}
