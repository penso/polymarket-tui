//! TUI for browsing trending events with live trade monitoring

use chrono::{DateTime, Utc};
use polymarket_bot::clob::ClobClient;
use polymarket_bot::gamma::Event;
use polymarket_bot::rtds::RTDSMessage;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, Wrap,
    },
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;

#[derive(Debug)]
pub struct Trade {
    pub timestamp: i64,
    pub side: String,
    pub outcome: String,
    pub price: f64,
    pub shares: f64,
    pub total_value: f64,
    pub title: String,
    pub user: String,
    pub pseudonym: String,
}

#[derive(Debug)]
pub struct EventTrades {
    pub trades: Vec<Trade>,
    pub is_watching: bool,
}

impl EventTrades {
    pub fn new() -> Self {
        Self {
            trades: Vec::new(),
            is_watching: false,
        }
    }

    pub fn add_trade(&mut self, msg: &RTDSMessage) {
        let rounded_shares = (msg.payload.size * 100.0).round() / 100.0;
        let total_value = msg.payload.price * msg.payload.size;

        let trade = Trade {
            timestamp: msg.payload.timestamp,
            side: msg.payload.side.clone(),
            outcome: msg.payload.outcome.clone(),
            price: msg.payload.price,
            shares: rounded_shares,
            total_value,
            title: msg.payload.title.clone(),
            user: msg.payload.name.clone(),
            pseudonym: msg.payload.pseudonym.clone(),
        };

        self.trades.insert(0, trade);
        // Keep only the last 500 trades per event
        if self.trades.len() > 500 {
            self.trades.truncate(500);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPanel {
    Header,       // Top panel with filter options
    EventsList,   // Left panel with events
    EventDetails, // Right panel - event details
    Markets,      // Right panel - markets
    Trades,       // Right panel - trades
    Logs,         // Bottom panel - logs
}

/// Event filter type for different views
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventFilter {
    Trending, // Order by volume24hr (default)
    Breaking, // Order by startTime
    New,      // Order by creationTime/createdAt
}

impl EventFilter {
    pub fn order_by(&self) -> &'static str {
        match self {
            EventFilter::Trending => "volume24hr",
            EventFilter::Breaking => "startTime",
            EventFilter::New => "creationTime",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            EventFilter::Trending => "Trending",
            EventFilter::Breaking => "Breaking",
            EventFilter::New => "New",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            EventFilter::Trending => EventFilter::Breaking,
            EventFilter::Breaking => EventFilter::New,
            EventFilter::New => EventFilter::Trending,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            EventFilter::Trending => EventFilter::New,
            EventFilter::Breaking => EventFilter::Trending,
            EventFilter::New => EventFilter::Breaking,
        }
    }
}

/// Search mode enum to replace boolean flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    None,        // No search/filter active
    ApiSearch,   // API search mode (triggered by '/')
    LocalFilter, // Local filter mode (triggered by 'f')
}

/// Search-related state
#[derive(Debug)]
pub struct SearchState {
    pub mode: SearchMode,
    pub query: String,
    pub results: Vec<Event>,         // Results from API search
    pub is_searching: bool,          // Whether a search API call is in progress
    pub last_searched_query: String, // Last query that was searched
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            mode: SearchMode::None,
            query: String::new(),
            results: Vec::new(),
            is_searching: false,
            last_searched_query: String::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.mode != SearchMode::None
    }
}

/// Scroll positions for all panels
#[derive(Debug)]
pub struct ScrollState {
    pub events_list: usize,   // Scroll position for events list
    pub markets: usize,       // Scroll position for markets panel
    pub trades: usize,        // Scroll position for trades table
    pub event_details: usize, // Scroll position for event details
    pub logs: usize,          // Scroll position for logs panel
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            events_list: 0,
            markets: 0,
            trades: 0,
            event_details: 0,
            logs: 0,
        }
    }
}

/// Pagination and infinite scrolling state
#[derive(Debug)]
pub struct PaginationState {
    pub current_limit: usize,   // Current number of events fetched
    pub is_fetching_more: bool, // Whether we're currently fetching more events
    pub order_by: String,       // Order by parameter for API calls
    pub ascending: bool,        // Ascending parameter for API calls
}

impl PaginationState {
    pub fn new(order_by: String, ascending: bool, initial_limit: usize) -> Self {
        Self {
            current_limit: initial_limit,
            is_fetching_more: false,
            order_by,
            ascending,
        }
    }
}

/// Logs state
#[derive(Debug)]
pub struct LogsState {
    pub messages: Vec<String>,
    pub scroll: usize,
}

impl LogsState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll: 0,
        }
    }
}

/// Navigation state (selection and focus)
#[derive(Debug)]
pub struct NavigationState {
    pub selected_index: usize,
    pub focused_panel: FocusedPanel,
}

impl NavigationState {
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            focused_panel: FocusedPanel::Header, // Start with Header focused so users can see filter options
        }
    }
}

/// Trades and WebSocket management state
#[derive(Debug)]
pub struct TradesState {
    // Map from event slug to trades
    pub event_trades: HashMap<String, EventTrades>,
    // Map from event slug to websocket task handle
    pub ws_handles: HashMap<String, JoinHandle<()>>,
}

impl TradesState {
    pub fn new() -> Self {
        Self {
            event_trades: HashMap::new(),
            ws_handles: HashMap::new(),
        }
    }
}

/// Main application state
pub struct TrendingAppState {
    pub events: Vec<Event>,
    pub should_quit: bool,
    pub search: SearchState,
    pub scroll: ScrollState,
    pub pagination: PaginationState,
    pub logs: LogsState,
    pub navigation: NavigationState,
    pub trades: TradesState,
    pub event_filter: EventFilter, // Current filter (Trending, Breaking, New)
    pub market_prices: HashMap<String, f64>, // asset_id -> current price from API
}

impl TrendingAppState {
    pub fn new(events: Vec<Event>, order_by: String, ascending: bool) -> Self {
        let current_limit = events.len();
        // Determine initial filter based on order_by
        let event_filter = if order_by == "startTime" {
            EventFilter::Breaking
        } else if order_by == "creationTime" || order_by == "createdAt" {
            EventFilter::New
        } else {
            EventFilter::Trending
        };
        Self {
            events,
            should_quit: false,
            search: SearchState::new(),
            scroll: ScrollState::new(),
            pagination: PaginationState::new(order_by, ascending, current_limit),
            logs: LogsState::new(),
            navigation: NavigationState::new(),
            trades: TradesState::new(),
            event_filter,
            market_prices: HashMap::new(),
        }
    }

    /// Check if we need to fetch more events (when user is near the end)
    pub fn should_fetch_more(&self) -> bool {
        // Only fetch more if not in search/filter mode and not already fetching
        if self.search.is_active()
            || !self.search.query.is_empty()
            || self.pagination.is_fetching_more
        {
            return false;
        }

        let filtered_len = self.filtered_events().len();
        // Fetch more when user is within 5 items of the end
        self.navigation.selected_index >= filtered_len.saturating_sub(5)
            && filtered_len >= self.pagination.current_limit
    }

    pub fn add_log(&mut self, level: &str, message: String) {
        // Format: [LEVEL] message
        let formatted = format!("[{}] {}", level, message);
        self.logs.messages.push(formatted);
        // Keep only last 1000 logs
        if self.logs.messages.len() > 1000 {
            self.logs.messages.remove(0);
        }
        // Auto-scroll to bottom - always show the latest logs
        // The logs area is Constraint::Length(8), so visible height is ~6 lines (minus borders)
        // We'll set scroll to show from the bottom, and render_logs will adjust if needed
        let estimated_visible_height = 6; // Approximate visible lines (8 - 2 for borders)
        if self.logs.messages.len() > estimated_visible_height {
            self.logs.scroll = self.logs.messages.len() - estimated_visible_height;
        } else {
            self.logs.scroll = 0;
        }
    }

    /// Get filtered events based on search query
    /// If in local filter mode, always filter locally from current list
    /// If in API search mode and results are available, use those
    /// Otherwise filter locally
    pub fn filtered_events(&self) -> Vec<&Event> {
        if self.search.query.is_empty() {
            // No query, return all events from the current source
            // If we have search results and not in local filter mode, return those; otherwise return all events
            if !self.search.results.is_empty()
                && self.search.mode != SearchMode::LocalFilter
                && self.search.mode == SearchMode::ApiSearch
            {
                return self.search.results.iter().collect();
            }
            return self.events.iter().collect();
        }

        // If in local filter mode, always filter from the source list
        if self.search.mode == SearchMode::LocalFilter {
            // Determine source list: if we have search results, filter from those; otherwise filter from events
            let query_lower = self.search.query.to_lowercase();
            if !self.search.results.is_empty() {
                // Filter from search results (current displayed list)
                return self
                    .search
                    .results
                    .iter()
                    .filter(|event| {
                        event.title.to_lowercase().contains(&query_lower)
                            || event.slug.to_lowercase().contains(&query_lower)
                            || event
                                .tags
                                .iter()
                                .any(|tag| tag.label.to_lowercase().contains(&query_lower))
                            || event
                                .markets
                                .iter()
                                .any(|market| market.question.to_lowercase().contains(&query_lower))
                    })
                    .collect();
            } else {
                // Filter from events list
                return self
                    .events
                    .iter()
                    .filter(|event| {
                        event.title.to_lowercase().contains(&query_lower)
                            || event.slug.to_lowercase().contains(&query_lower)
                            || event
                                .tags
                                .iter()
                                .any(|tag| tag.label.to_lowercase().contains(&query_lower))
                            || event
                                .markets
                                .iter()
                                .any(|market| market.question.to_lowercase().contains(&query_lower))
                    })
                    .collect();
            }
        }

        // API search mode: use API results if available
        if !self.search.results.is_empty() && self.search.query == self.search.last_searched_query {
            return self.search.results.iter().collect();
        }

        // Fall back to local filtering
        let query_lower = self.search.query.to_lowercase();
        self.events
            .iter()
            .filter(|event| {
                event.title.to_lowercase().contains(&query_lower)
                    || event.slug.to_lowercase().contains(&query_lower)
                    || event
                        .tags
                        .iter()
                        .any(|tag| tag.label.to_lowercase().contains(&query_lower))
                    || event
                        .markets
                        .iter()
                        .any(|market| market.question.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Get the currently selected event from filtered list
    pub fn selected_event_filtered(&self) -> Option<&Event> {
        let filtered = self.filtered_events();
        filtered.get(self.navigation.selected_index).copied()
    }

    pub fn enter_search_mode(&mut self) {
        self.search.mode = SearchMode::ApiSearch;
        self.search.query.clear();
    }

    pub fn enter_local_filter_mode(&mut self) {
        self.search.mode = SearchMode::LocalFilter;
        self.search.query.clear();
    }

    pub fn exit_search_mode(&mut self) {
        self.search.mode = SearchMode::None;
        self.search.query.clear();
        self.navigation.selected_index = 0;
        self.scroll.events_list = 0;
    }

    pub fn is_in_filter_mode(&self) -> bool {
        self.search.is_active()
    }

    pub fn add_search_char(&mut self, c: char) {
        self.search.query.push(c);
        self.navigation.selected_index = 0;
        self.scroll.events_list = 0;
    }

    pub fn delete_search_char(&mut self) {
        self.search.query.pop();
        self.navigation.selected_index = 0;
        self.scroll.events_list = 0;
        // Clear search results when query changes
        if self.search.query != self.search.last_searched_query {
            self.search.results.clear();
        }
    }

    pub fn set_search_results(&mut self, results: Vec<Event>, query: String) {
        self.search.results = results;
        self.search.last_searched_query = query;
        self.search.is_searching = false;
        self.navigation.selected_index = 0;
        self.scroll.events_list = 0;
    }

    pub fn set_searching(&mut self, searching: bool) {
        self.search.is_searching = searching;
    }

    pub fn selected_event(&self) -> Option<&Event> {
        // Always use filtered events to ensure we get the event from the currently displayed list
        // This works for:
        // - Local filter mode (filters current list)
        // - API search mode (uses search results)
        // - Normal mode (uses all events)
        // Using filtered_events() ensures consistency between what's displayed and what's selected
        self.selected_event_filtered()
    }

    pub fn selected_event_slug(&self) -> Option<String> {
        self.selected_event().map(|e| e.slug.clone())
    }

    pub fn move_up(&mut self) {
        if self.navigation.selected_index > 0 {
            self.navigation.selected_index -= 1;
            if self.navigation.selected_index < self.scroll.events_list {
                self.scroll.events_list = self.navigation.selected_index;
            }
        }
    }

    pub fn move_down(&mut self) {
        let filtered_len = self.filtered_events().len();
        if self.navigation.selected_index < filtered_len.saturating_sub(1) {
            self.navigation.selected_index += 1;
            let visible_height = 20;
            if self.navigation.selected_index >= self.scroll.events_list + visible_height {
                self.scroll.events_list = self.navigation.selected_index - visible_height + 1;
            }
        }
    }

    pub fn is_watching(&self, event_slug: &str) -> bool {
        self.trades
            .event_trades
            .get(event_slug)
            .map(|et| et.is_watching)
            .unwrap_or(false)
    }

    pub fn get_trades(&self, event_slug: &str) -> &[Trade] {
        self.trades
            .event_trades
            .get(event_slug)
            .map(|et| et.trades.as_slice())
            .unwrap_or(&[])
    }

    pub fn start_watching(&mut self, event_slug: String, ws_handle: JoinHandle<()>) {
        self.trades
            .event_trades
            .entry(event_slug.clone())
            .or_insert_with(EventTrades::new)
            .is_watching = true;
        self.trades.ws_handles.insert(event_slug, ws_handle);
    }

    pub fn stop_watching(&mut self, event_slug: &str) {
        if let Some(handle) = self.trades.ws_handles.remove(event_slug) {
            handle.abort();
        }
        if let Some(event_trades) = self.trades.event_trades.get_mut(event_slug) {
            event_trades.is_watching = false;
        }
    }

    pub fn cleanup(&mut self) {
        for handle in self.trades.ws_handles.values() {
            handle.abort();
        }
        self.trades.ws_handles.clear();
    }
}

pub fn render(f: &mut Frame, app: &mut TrendingAppState) {
    // Header height: 3 lines for normal mode (title, filters, info), 6 for search mode
    let header_height = if app.is_in_filter_mode() { 6 } else { 4 };
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
            filtered_count,
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

    // Footer - show focused panel info
    let focused_panel_text = match app.navigation.focused_panel {
        FocusedPanel::Header => "Filter",
        FocusedPanel::EventsList => "Events List",
        FocusedPanel::EventDetails => "Event Details",
        FocusedPanel::Markets => "Markets",
        FocusedPanel::Trades => "Trades",
        FocusedPanel::Logs => "Logs",
    };
    let footer_text = if app.search.mode == SearchMode::ApiSearch {
        format!(
            "Type to search | Esc to exit | Focused: {} | 'q' to quit",
            focused_panel_text
        )
    } else {
        format!("Press '/' to search | Tab to switch panels | Focused: {} | ↑↓ to scroll | Enter to watch/unwatch | 'q' to quit", focused_panel_text)
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
            let right_text_width = right_text.len();

            // Reserve space for right text + 1 space padding
            let reserved_width = right_text_width + 1;
            let available_width = usable_width.saturating_sub(reserved_width);

            // Truncate title to fit available space
            let title = if event.title.len() > available_width {
                truncate(&event.title, available_width.saturating_sub(3))
            } else {
                event.title.clone()
            };

            let title_width = title.len();
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

                    let title_truncated = truncate(&trade.title, 30);
                    let user_truncated = truncate(&trade.user, 15);
                    let side_text = if trade.side == "BUY" {
                        "🟢 BUY".to_string()
                    } else {
                        "🔴 SELL".to_string()
                    };
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

            let table = Table::new(
                rows,
                [
                    Constraint::Length(10),     // Time
                    Constraint::Length(8),      // Side
                    Constraint::Length(5),      // Outcome
                    Constraint::Length(10),     // Price
                    Constraint::Length(10),     // Shares
                    Constraint::Length(10),     // Value
                    Constraint::Percentage(30), // Title
                    Constraint::Length(15),     // User
                ],
            )
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
    // Calculate total volume from all markets
    let total_volume: f64 = event
        .markets
        .iter()
        .map(|m| m.volume_total.unwrap_or(0.0))
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
            if event.active { "Active" } else { "Inactive" },
            Style::default().fg(if event.active {
                Color::Green
            } else {
                Color::Red
            }),
        ),
        Span::styled(" | ", Style::default().fg(Color::Gray)),
        Span::styled(
            if event.closed { "Closed" } else { "Open" },
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
            Style::default().fg(if is_watching { Color::Red } else { Color::Gray }),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Estimated End: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(end_date_str, Style::default().fg(Color::Magenta)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Total Volume: ", Style::default().fg(Color::Yellow).bold()),
        Span::styled(
            format!("${:.2}", total_volume),
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

    // Create list items for markets with scroll
    let items: Vec<ListItem> = event
        .markets
        .iter()
        .skip(scroll)
        .take(visible_height)
        .map(|market| {
            let volume_str = market
                .volume_total
                .map(|v| format!(" ${:.2}", v))
                .unwrap_or_default();

            // Build outcome strings with prices and percentages from API
            let mut outcome_strings = Vec::new();
            for (idx, outcome) in market.outcomes.iter().enumerate() {
                // Try to get price from API-fetched market_prices first
                let price = if let Some(ref token_ids) = market.clob_token_ids {
                    token_ids
                        .get(idx)
                        .and_then(|asset_id| app.market_prices.get(asset_id).copied())
                        .or_else(|| {
                            // Fallback to outcome_prices if API price not available
                            market
                                .outcome_prices
                                .get(idx)
                                .and_then(|p| p.parse::<f64>().ok())
                        })
                } else {
                    // Fallback to outcome_prices if no token IDs
                    market
                        .outcome_prices
                        .get(idx)
                        .and_then(|p| p.parse::<f64>().ok())
                };

                let price_str = price
                    .map(|p| {
                        let percent = p * 100.0;
                        format!("${:.3} ({:.1}%)", p, percent)
                    })
                    .unwrap_or_else(|| "N/A".to_string());

                outcome_strings.push(format!("{}: {}", outcome, price_str));
            }

            let outcomes_str = if !outcome_strings.is_empty() {
                outcome_strings.join(" | ")
            } else {
                String::new()
            };

            // Calculate widths for right alignment
            let usable_width = (area.width as usize).saturating_sub(2); // -2 for borders

            // Calculate space needed for outcomes and volume (right-aligned)
            let outcomes_width = outcomes_str.len();
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

            // Calculate available width for question (reserve space for right content + 1 space padding)
            let available_width = usable_width
                .saturating_sub(right_content_width)
                .saturating_sub(1); // 1 space padding between question and right content

            // Truncate question to fit available width
            let question = truncate(&market.question, available_width);
            let question_width = question.len();

            // Calculate remaining width for spacing
            let remaining_width = usable_width
                .saturating_sub(question_width)
                .saturating_sub(right_content_width);

            let mut line_spans = vec![Span::styled(question, Style::default().fg(Color::White))];

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

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
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
                .title(if is_focused { "Logs (Focused)" } else { "Logs" })
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

pub async fn run_trending_tui(
    mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
    app_state: Arc<TokioMutex<TrendingAppState>>,
) -> anyhow::Result<Option<String>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use polymarket_bot::{GammaClient, RTDSClient};

    let mut search_debounce: Option<tokio::time::Instant> = None;
    let mut last_selected_event_slug: Option<String> = None;

    loop {
        // Handle search debouncing and API calls
        // Check debounce timer and trigger search if needed
        if let Some(debounce_time) = search_debounce {
            let elapsed = debounce_time.elapsed();
            if elapsed >= tokio::time::Duration::from_millis(500) {
                // Debounce period passed, perform search
                let query = {
                    let app = app_state.lock().await;
                    app.search.query.clone()
                };

                // Clear debounce before processing to prevent race conditions
                search_debounce = None;

                if !query.is_empty() {
                    // Search for any non-empty query
                    tracing::info!("Searching for: '{}'", query);
                    let app_state_clone = Arc::clone(&app_state);
                    let query_clone = query.clone();
                    // Create a new GammaClient for the async task
                    let gamma_client_for_task = GammaClient::new();

                    {
                        let mut app = app_state.lock().await;
                        app.set_searching(true);
                    }

                    // Spawn the search task
                    // The tracing context should be inherited automatically since we're using set_default()
                    tokio::spawn(async move {
                        // Test log to verify tracing works in spawned task
                        tracing::info!("[TASK] Starting search for: '{}'", query_clone);

                        let result = gamma_client_for_task
                            .search_events(&query_clone, Some(50))
                            .await;

                        match result {
                            Ok(results) => {
                                tracing::info!("Search found {} results", results.len());
                                let mut app = app_state_clone.lock().await;
                                app.set_search_results(results, query_clone);
                            }
                            Err(e) => {
                                tracing::error!("Search failed: {}", e);
                                let mut app = app_state_clone.lock().await;
                                app.set_searching(false);
                                app.search.results.clear();
                            }
                        }
                    });
                } else {
                    // Query is empty, clear search results
                    let mut app = app_state.lock().await;
                    app.search.results.clear();
                    app.search.last_searched_query.clear();
                    app.set_searching(false);
                }
            }
        }

        {
            let mut app = app_state.lock().await;
            terminal.draw(|f| {
                render(f, &mut app);
            })?;
        }

        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let mut app = app_state.lock().await;
                    match key.code {
                        KeyCode::Char('q') => {
                            if app.is_in_filter_mode() {
                                app.exit_search_mode();
                            } else {
                                app.should_quit = true;
                                break;
                            }
                        }
                        KeyCode::Esc => {
                            if app.is_in_filter_mode() {
                                app.exit_search_mode();
                            } else {
                                app.should_quit = true;
                                break;
                            }
                        }
                        KeyCode::Char('/') => {
                            if !app.is_in_filter_mode() {
                                app.enter_search_mode();
                            }
                        }
                        KeyCode::Char('f') => {
                            if !app.is_in_filter_mode() {
                                app.enter_local_filter_mode();
                            }
                        }
                        KeyCode::Tab => {
                            if !app.is_in_filter_mode() {
                                // Cycle through panels: Header -> EventsList -> EventDetails -> Markets -> Trades -> Logs -> Header
                                app.navigation.focused_panel = match app.navigation.focused_panel {
                                    FocusedPanel::Header => FocusedPanel::EventsList,
                                    FocusedPanel::EventsList => FocusedPanel::EventDetails,
                                    FocusedPanel::EventDetails => FocusedPanel::Markets,
                                    FocusedPanel::Markets => FocusedPanel::Trades,
                                    FocusedPanel::Trades => FocusedPanel::Logs,
                                    FocusedPanel::Logs => FocusedPanel::Header,
                                };
                            }
                        }
                        KeyCode::Left => {
                            if !app.is_in_filter_mode()
                                && app.navigation.focused_panel == FocusedPanel::Header
                            {
                                // Switch to previous filter
                                let old_filter = app.event_filter;
                                app.event_filter = app.event_filter.prev();

                                // If filter changed, trigger refetch
                                if old_filter != app.event_filter {
                                    // Clear search results when filter changes
                                    app.search.results.clear();
                                    app.search.last_searched_query.clear();

                                    let app_state_clone = Arc::clone(&app_state);
                                    let gamma_client_clone = GammaClient::new();
                                    let order_by = app.event_filter.order_by().to_string();
                                    let ascending = false; // Always descending for these views
                                    let limit = app.pagination.current_limit;

                                    app.pagination.order_by = order_by.clone();
                                    app.pagination.is_fetching_more = true;

                                    tracing::info!(
                                        "Switching to {} filter, fetching events...",
                                        app.event_filter.label()
                                    );

                                    tokio::spawn(async move {
                                        match gamma_client_clone
                                            .get_trending_events(
                                                Some(&order_by),
                                                Some(ascending),
                                                Some(limit),
                                            )
                                            .await
                                        {
                                            Ok(new_events) => {
                                                tracing::info!(
                                                    "Fetched {} events for {} filter",
                                                    new_events.len(),
                                                    order_by
                                                );
                                                let mut app = app_state_clone.lock().await;
                                                app.events = new_events;
                                                app.pagination.is_fetching_more = false;
                                                app.navigation.selected_index = 0;
                                                app.scroll.events_list = 0;
                                            }
                                            Err(e) => {
                                                tracing::error!("Failed to fetch events: {}", e);
                                                let mut app = app_state_clone.lock().await;
                                                app.pagination.is_fetching_more = false;
                                            }
                                        }
                                    });
                                }
                            }
                        }
                        KeyCode::Right => {
                            if !app.is_in_filter_mode()
                                && app.navigation.focused_panel == FocusedPanel::Header
                            {
                                // Switch to next filter
                                let old_filter = app.event_filter;
                                app.event_filter = app.event_filter.next();

                                // If filter changed, trigger refetch
                                if old_filter != app.event_filter {
                                    // Clear search results when filter changes
                                    app.search.results.clear();
                                    app.search.last_searched_query.clear();

                                    let app_state_clone = Arc::clone(&app_state);
                                    let gamma_client_clone = GammaClient::new();
                                    let order_by = app.event_filter.order_by().to_string();
                                    let ascending = false; // Always descending for these views
                                    let limit = app.pagination.current_limit;

                                    app.pagination.order_by = order_by.clone();
                                    app.pagination.is_fetching_more = true;

                                    tracing::info!(
                                        "Switching to {} filter, fetching events...",
                                        app.event_filter.label()
                                    );

                                    tokio::spawn(async move {
                                        match gamma_client_clone
                                            .get_trending_events(
                                                Some(&order_by),
                                                Some(ascending),
                                                Some(limit),
                                            )
                                            .await
                                        {
                                            Ok(new_events) => {
                                                tracing::info!(
                                                    "Fetched {} events for {} filter",
                                                    new_events.len(),
                                                    order_by
                                                );
                                                let mut app = app_state_clone.lock().await;
                                                app.events = new_events;
                                                app.pagination.is_fetching_more = false;
                                                app.navigation.selected_index = 0;
                                                app.scroll.events_list = 0;
                                            }
                                            Err(e) => {
                                                tracing::error!("Failed to fetch events: {}", e);
                                                let mut app = app_state_clone.lock().await;
                                                app.pagination.is_fetching_more = false;
                                            }
                                        }
                                    });
                                }
                            }
                        }
                        KeyCode::Up => {
                            if !app.is_in_filter_mode() {
                                match app.navigation.focused_panel {
                                    FocusedPanel::Header => {
                                        // Header doesn't scroll, but we can allow it for consistency
                                    }
                                    FocusedPanel::EventsList => {
                                        app.move_up();
                                        // Fetch market prices when event selection changes
                                        if let Some(event) = app.selected_event() {
                                            let current_slug = event.slug.clone();
                                            if last_selected_event_slug.as_ref()
                                                != Some(&current_slug)
                                            {
                                                last_selected_event_slug = Some(current_slug);
                                                let app_state_clone = Arc::clone(&app_state);
                                                let clob_client = ClobClient::new();
                                                let markets_clone: Vec<_> = event
                                                    .markets
                                                    .iter()
                                                    .map(|m| m.clob_token_ids.clone())
                                                    .collect();

                                                tokio::spawn(async move {
                                                    let mut prices = HashMap::new();
                                                    for token_ids in markets_clone {
                                                        if let Some(ref token_ids) = token_ids {
                                                            for asset_id in token_ids {
                                                                match clob_client
                                                                    .get_orderbook_by_asset(
                                                                        asset_id,
                                                                    )
                                                                    .await
                                                                {
                                                                    Ok(orderbook) => {
                                                                        // Get best ask price (price to buy)
                                                                        if let Some(best_ask) =
                                                                            orderbook.asks.first()
                                                                        {
                                                                            if let Ok(price) =
                                                                                best_ask
                                                                                    .price
                                                                                    .parse::<f64>()
                                                                            {
                                                                                prices.insert(
                                                                                    asset_id
                                                                                        .clone(),
                                                                                    price,
                                                                                );
                                                                                tracing::debug!("Fetched price for asset {}: ${:.3}", asset_id, price);
                                                                            }
                                                                        }
                                                                    }
                                                                    Err(e) => {
                                                                        // Only log as debug to reduce noise - empty orderbooks are common
                                                                        tracing::debug!("Failed to fetch orderbook for asset {}: {}", asset_id, e);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    let mut app = app_state_clone.lock().await;
                                                    app.market_prices.extend(prices);
                                                });
                                            }
                                        }
                                    }
                                    FocusedPanel::EventDetails => {
                                        if app.scroll.event_details > 0 {
                                            app.scroll.event_details -= 1;
                                        }
                                    }
                                    FocusedPanel::Markets => {
                                        if app.scroll.markets > 0 {
                                            app.scroll.markets -= 1;
                                        }
                                    }
                                    FocusedPanel::Trades => {
                                        if app.scroll.trades > 0 {
                                            app.scroll.trades -= 1;
                                        }
                                    }
                                    FocusedPanel::Logs => {
                                        if app.logs.scroll > 0 {
                                            app.logs.scroll -= 1;
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Down => {
                            if !app.is_in_filter_mode() {
                                match app.navigation.focused_panel {
                                    FocusedPanel::Header => {
                                        // Header doesn't scroll, but we can allow it for consistency
                                    }
                                    FocusedPanel::EventsList => {
                                        app.move_down();
                                        // Fetch market prices when event selection changes
                                        if let Some(event) = app.selected_event() {
                                            let current_slug = event.slug.clone();
                                            if last_selected_event_slug.as_ref()
                                                != Some(&current_slug)
                                            {
                                                last_selected_event_slug = Some(current_slug);
                                                let app_state_clone = Arc::clone(&app_state);
                                                let clob_client = ClobClient::new();
                                                let markets_clone: Vec<_> = event
                                                    .markets
                                                    .iter()
                                                    .map(|m| m.clob_token_ids.clone())
                                                    .collect();

                                                tokio::spawn(async move {
                                                    let mut prices = HashMap::new();
                                                    for token_ids in markets_clone {
                                                        if let Some(ref token_ids) = token_ids {
                                                            for asset_id in token_ids {
                                                                match clob_client
                                                                    .get_orderbook_by_asset(
                                                                        asset_id,
                                                                    )
                                                                    .await
                                                                {
                                                                    Ok(orderbook) => {
                                                                        // Get best ask price (price to buy)
                                                                        if let Some(best_ask) =
                                                                            orderbook.asks.first()
                                                                        {
                                                                            if let Ok(price) =
                                                                                best_ask
                                                                                    .price
                                                                                    .parse::<f64>()
                                                                            {
                                                                                prices.insert(
                                                                                    asset_id
                                                                                        .clone(),
                                                                                    price,
                                                                                );
                                                                                tracing::debug!("Fetched price for asset {}: ${:.3}", asset_id, price);
                                                                            }
                                                                        }
                                                                    }
                                                                    Err(e) => {
                                                                        // Only log as debug to reduce noise - empty orderbooks are common
                                                                        tracing::debug!("Failed to fetch orderbook for asset {}: {}", asset_id, e);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    let mut app = app_state_clone.lock().await;
                                                    app.market_prices.extend(prices);
                                                });
                                            }
                                        }
                                        // Check if we need to fetch more events (infinite scroll)
                                        if app.should_fetch_more() {
                                            let app_state_clone = Arc::clone(&app_state);
                                            let gamma_client_clone = GammaClient::new();
                                            let order_by = app.pagination.order_by.clone();
                                            let ascending = app.pagination.ascending;
                                            let current_limit = app.pagination.current_limit;

                                            // Set fetching flag to prevent duplicate requests
                                            app.pagination.is_fetching_more = true;

                                            // Fetch 50 more events
                                            let new_limit = current_limit + 50;
                                            tracing::info!(
                                                "Fetching more trending events (limit: {})",
                                                new_limit
                                            );

                                            tokio::spawn(async move {
                                                match gamma_client_clone
                                                    .get_trending_events(
                                                        Some(&order_by),
                                                        Some(ascending),
                                                        Some(new_limit),
                                                    )
                                                    .await
                                                {
                                                    Ok(mut new_events) => {
                                                        // Remove duplicates by comparing slugs
                                                        let existing_slugs: std::collections::HashSet<_> = {
                                                    let app = app_state_clone.lock().await;
                                                    app.events
                                                        .iter()
                                                        .map(|e| e.slug.clone())
                                                        .collect()
                                                };

                                                        new_events.retain(|e| {
                                                            !existing_slugs.contains(&e.slug)
                                                        });

                                                        if !new_events.is_empty() {
                                                            tracing::info!(
                                                                "Fetched {} new trending events",
                                                                new_events.len()
                                                            );
                                                            let mut app =
                                                                app_state_clone.lock().await;
                                                            app.events.append(&mut new_events);
                                                            app.pagination.current_limit =
                                                                new_limit;
                                                        } else {
                                                            tracing::info!("No new events to add (already have all events)");
                                                        }

                                                        let mut app = app_state_clone.lock().await;
                                                        app.pagination.is_fetching_more = false;
                                                    }
                                                    Err(e) => {
                                                        tracing::error!(
                                                            "Failed to fetch more events: {}",
                                                            e
                                                        );
                                                        let mut app = app_state_clone.lock().await;
                                                        app.pagination.is_fetching_more = false;
                                                    }
                                                }
                                            });
                                        }
                                    }
                                    FocusedPanel::EventDetails => {
                                        // Calculate actual content height for event details
                                        if let Some(event) = app.selected_event() {
                                            // Base lines: Title, Slug, Event ID, Status, Estimated End, Total Volume
                                            let mut total_lines = 6;

                                            // Calculate wrapped tags lines
                                            if !event.tags.is_empty() {
                                                let tag_labels: Vec<String> = event
                                                    .tags
                                                    .iter()
                                                    .map(|tag| truncate(&tag.label, 20))
                                                    .collect();
                                                let tags_text = tag_labels.join(", ");
                                                // Approximate available width (will be calculated more accurately in render)
                                                // Assume ~60 chars available for tags content
                                                let tags_content_width = 60;
                                                if tags_text.len() > tags_content_width {
                                                    // Tags wrap - calculate how many lines
                                                    let wrapped_lines = tags_text
                                                        .len()
                                                        .div_ceil(tags_content_width);
                                                    total_lines += wrapped_lines;
                                                } else {
                                                    total_lines += 1; // Single line for tags
                                                }
                                            }

                                            // Get visible height from the actual area (approximate)
                                            let visible_height: usize = 6; // Minimum height minus borders
                                            let max_scroll =
                                                total_lines.saturating_sub(visible_height.max(1));
                                            if app.scroll.event_details < max_scroll {
                                                app.scroll.event_details += 1;
                                            }
                                        }
                                    }
                                    FocusedPanel::Markets => {
                                        if let Some(event) = app.selected_event() {
                                            let visible_height: usize = 5; // Markets panel height
                                            if app.scroll.markets
                                                < event.markets.len().saturating_sub(visible_height)
                                            {
                                                app.scroll.markets += 1;
                                            }
                                        }
                                    }
                                    FocusedPanel::Trades => {
                                        let trades_len = if let Some(event) = app.selected_event() {
                                            app.get_trades(&event.slug).len()
                                        } else {
                                            0
                                        };
                                        let visible_height: usize = 10; // Approximate
                                        if app.scroll.trades
                                            < trades_len.saturating_sub(visible_height)
                                        {
                                            app.scroll.trades += 1;
                                        }
                                    }
                                    FocusedPanel::Logs => {
                                        // Calculate max scroll based on visible height (approximate)
                                        // The render function will clamp it to the exact visible height
                                        let visible_height: usize = 10; // Approximate, will be clamped in render
                                        let max_scroll = app
                                            .logs
                                            .messages
                                            .len()
                                            .saturating_sub(visible_height.max(1));
                                        if app.logs.scroll < max_scroll {
                                            app.logs.scroll += 1;
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            if app.is_in_filter_mode() {
                                app.delete_search_char();
                                // Trigger API search after backspace only if in API search mode (with debounce)
                                if app.search.mode == SearchMode::ApiSearch {
                                    search_debounce = Some(tokio::time::Instant::now());
                                }
                            }
                        }
                        KeyCode::Char(c) => {
                            if app.is_in_filter_mode() {
                                app.add_search_char(c);
                                // Trigger API search after character input only if in API search mode (with debounce)
                                if app.search.mode == SearchMode::ApiSearch {
                                    search_debounce = Some(tokio::time::Instant::now());
                                }
                                // Local filter mode filters immediately (no API call needed)
                            }
                        }
                        KeyCode::Enter => {
                            if app.is_in_filter_mode() {
                                // Exit search/filter mode and keep selection
                                app.search.mode = SearchMode::None;
                            } else {
                                // Toggle watching the selected event
                                if let Some(event_slug) = app.selected_event_slug() {
                                    if app.is_watching(&event_slug) {
                                        // Stop watching
                                        app.stop_watching(&event_slug);
                                    } else {
                                        // Start watching
                                        let event_slug_clone = event_slug.clone();

                                        // Ensure the event_trades entry exists before starting websocket
                                        app.trades
                                            .event_trades
                                            .entry(event_slug_clone.clone())
                                            .or_insert_with(EventTrades::new);

                                        let app_state_ws = Arc::clone(&app_state);
                                        let event_slug_for_closure = event_slug_clone.clone();

                                        let rtds_client = RTDSClient::new()
                                            .with_event_slug(event_slug_clone.clone());
                                        let event_slug_for_log = event_slug_clone.clone();

                                        tracing::info!(
                                            "Starting RTDS WebSocket for event: {}",
                                            event_slug_clone
                                        );

                                        let ws_handle = tokio::spawn(async move {
                                            match rtds_client
                                                .connect_and_listen(move |msg| {
                                                    let app_state = Arc::clone(&app_state_ws);
                                                    let event_slug = event_slug_for_closure.clone();

                                                    tracing::info!("Received RTDS trade for event: {}", event_slug);

                                                    tokio::spawn(async move {
                                                        let mut app = app_state.lock().await;
                                                        if let Some(event_trades) =
                                                            app.trades.event_trades.get_mut(&event_slug)
                                                        {
                                                            event_trades.add_trade(&msg);
                                                            tracing::info!("Trade added to event_trades for: {}", event_slug);
                                                        } else {
                                                            tracing::warn!("No event_trades entry found for: {}", event_slug);
                                                        }
                                                    });
                                                })
                                                .await
                                            {
                                                Ok(()) => {
                                                    tracing::info!("RTDS WebSocket connection closed normally for event: {}", event_slug_for_log);
                                                }
                                                Err(e) => {
                                                    tracing::error!(
                                                        "RTDS WebSocket error for event {}: {}",
                                                        event_slug_for_log,
                                                        e
                                                    );
                                                }
                                            }
                                        });

                                        app.start_watching(event_slug_clone, ws_handle);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        {
            let app = app_state.lock().await;
            if app.should_quit {
                break;
            }
        }
    }

    // Cleanup
    {
        let mut app = app_state.lock().await;
        app.cleanup();
    }

    Ok(None)
}
