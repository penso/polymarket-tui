//! State types for the trending TUI

use polymarket_tui::gamma::Event;
use polymarket_tui::rtds::RTDSMessage;
use std::collections::HashMap;
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub logs: usize, // Scroll position for logs panel
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
