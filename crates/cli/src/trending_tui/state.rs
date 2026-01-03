//! State types for the trending TUI

use {
    chrono::{DateTime, Utc},
    polymarket_api::{gamma::Event, rtds::RTDSMessage},
    ratatui::widgets::TableState,
    std::collections::HashMap,
    tokio::task::JoinHandle,
};

#[derive(Debug)]
pub struct Trade {
    pub timestamp: i64,
    pub side: String,
    pub outcome: String,
    pub price: f64,
    pub shares: f64,
    pub total_value: f64,
    pub title: String,
    pub asset_id: String,
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
            asset_id: msg.payload.asset.clone(),
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

/// Main tab at the top level (Trending vs Yield)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MainTab {
    Trending,
    Yield,
}

impl MainTab {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            MainTab::Trending => "Trending",
            MainTab::Yield => "Yield",
        }
    }

    #[allow(dead_code)]
    pub fn next(&self) -> Self {
        match self {
            MainTab::Trending => MainTab::Yield,
            MainTab::Yield => MainTab::Trending,
        }
    }

    #[allow(dead_code)]
    pub fn prev(&self) -> Self {
        self.next() // Only 2 tabs, so prev == next
    }
}

/// Event filter type for different views
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventFilter {
    Trending, // Order by volume24hr (default)
    Breaking, // Order by startTime
    New,      // Order by creationTime/createdAt
}

impl EventFilter {
    pub fn order_by(&self) -> &'static str {
        match self {
            EventFilter::Trending => "volume24hr",
            EventFilter::Breaking => "startDate",
            EventFilter::New => "createdAt",
        }
    }

    #[allow(dead_code)]
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

/// Popup types for modal dialogs
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PopupType {
    Help,              // Show help/keyboard shortcuts
    ConfirmQuit,       // Confirm before quitting
    EventInfo(String), // Show detailed event info (slug)
    Login,             // Login modal with credential input
    UserProfile,       // Show authenticated user profile
    Trade(String),     // Trade modal for a market (token_id)
}

/// Login form field being edited
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    ApiKey,
    Secret,
    Passphrase,
    Address,
}

#[allow(dead_code)]
impl LoginField {
    pub fn next(&self) -> Self {
        match self {
            LoginField::ApiKey => LoginField::Secret,
            LoginField::Secret => LoginField::Passphrase,
            LoginField::Passphrase => LoginField::Address,
            LoginField::Address => LoginField::ApiKey,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            LoginField::ApiKey => LoginField::Address,
            LoginField::Secret => LoginField::ApiKey,
            LoginField::Passphrase => LoginField::Secret,
            LoginField::Address => LoginField::Passphrase,
        }
    }
}

/// Login form state
#[derive(Debug, Clone)]
pub struct LoginFormState {
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
    pub address: String,
    pub active_field: LoginField,
    pub error_message: Option<String>,
    pub is_validating: bool,
}

#[allow(dead_code)]
impl LoginFormState {
    pub fn new() -> Self {
        Self {
            api_key: String::new(),
            secret: String::new(),
            passphrase: String::new(),
            address: String::new(),
            active_field: LoginField::ApiKey,
            error_message: None,
            is_validating: false,
        }
    }

    pub fn get_active_field_value(&self) -> &str {
        match self.active_field {
            LoginField::ApiKey => &self.api_key,
            LoginField::Secret => &self.secret,
            LoginField::Passphrase => &self.passphrase,
            LoginField::Address => &self.address,
        }
    }

    pub fn add_char(&mut self, c: char) {
        match self.active_field {
            LoginField::ApiKey => self.api_key.push(c),
            LoginField::Secret => self.secret.push(c),
            LoginField::Passphrase => self.passphrase.push(c),
            LoginField::Address => self.address.push(c),
        }
        self.error_message = None;
    }

    pub fn delete_char(&mut self) {
        match self.active_field {
            LoginField::ApiKey => {
                self.api_key.pop();
            },
            LoginField::Secret => {
                self.secret.pop();
            },
            LoginField::Passphrase => {
                self.passphrase.pop();
            },
            LoginField::Address => {
                self.address.pop();
            },
        }
        self.error_message = None;
    }

    pub fn clear(&mut self) {
        self.api_key.clear();
        self.secret.clear();
        self.passphrase.clear();
        self.address.clear();
        self.active_field = LoginField::ApiKey;
        self.error_message = None;
        self.is_validating = false;
    }
}

/// User authentication state
#[derive(Debug, Clone)]
pub struct AuthState {
    pub is_authenticated: bool,
    pub username: Option<String>,
    pub address: Option<String>,
    pub balance: Option<f64>,
}

impl AuthState {
    pub fn new() -> Self {
        Self {
            is_authenticated: false,
            username: None,
            address: None,
            balance: None,
        }
    }

    pub fn display_name(&self) -> String {
        if let Some(ref name) = self.username {
            name.clone()
        } else if let Some(ref addr) = self.address {
            if addr.len() >= 10 {
                format!("{}...{}", &addr[..6], &addr[addr.len() - 4..])
            } else {
                addr.clone()
            }
        } else {
            "Unknown".to_string()
        }
    }
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
            focused_panel: FocusedPanel::EventsList, // Start with events list focused
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

/// A single yield opportunity (high probability market)
#[derive(Debug, Clone)]
pub struct YieldOpportunity {
    pub market_name: String,
    pub market_status: &'static str,
    pub outcome: String,
    pub price: f64,
    pub est_return: f64,
    pub volume: f64,
    pub event_slug: String,
    pub event_title: String,
    pub event_status: &'static str,
    pub end_date: Option<DateTime<Utc>>,
}

/// A search result in the Yield tab - an event with its best yield opportunity (if any)
#[derive(Debug, Clone)]
pub struct YieldSearchResult {
    pub event_slug: String,
    pub event_title: String,
    pub event_status: &'static str,
    pub end_date: Option<DateTime<Utc>>,
    pub total_volume: f64,
    pub markets_count: usize,
    /// Best yield opportunity for this event (highest return), if any
    pub best_yield: Option<YieldOpportunity>,
}

/// Yield tab state
#[derive(Debug)]
pub struct YieldState {
    pub opportunities: Vec<YieldOpportunity>,
    pub selected_index: usize,
    pub scroll: usize,
    pub is_loading: bool,
    pub min_prob: f64,
    pub min_volume: f64,
    pub sort_by: YieldSortBy,
    pub filter_query: String, // Current filter query
    pub is_filtering: bool,   // Whether filter input is active
    // API search state
    pub search_query: String,                   // Current search query
    pub search_results: Vec<YieldSearchResult>, // Search results with yield info
    pub is_searching: bool,                     // Whether search input is active
    pub is_search_loading: bool,                // Whether API search is in progress
    pub last_searched_query: String,            // Last query that was searched
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YieldSortBy {
    Return,  // Sort by estimated return (default)
    Volume,  // Sort by 24h volume
    EndDate, // Sort by end date (soonest first)
}

impl YieldSortBy {
    pub fn label(&self) -> &'static str {
        match self {
            YieldSortBy::Return => "Return",
            YieldSortBy::Volume => "Volume",
            YieldSortBy::EndDate => "End Date",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            YieldSortBy::Return => YieldSortBy::Volume,
            YieldSortBy::Volume => YieldSortBy::EndDate,
            YieldSortBy::EndDate => YieldSortBy::Return,
        }
    }
}

impl YieldState {
    pub fn new() -> Self {
        Self {
            opportunities: Vec::new(),
            selected_index: 0,
            scroll: 0,
            is_loading: false,
            min_prob: 0.95,
            min_volume: 0.0,
            sort_by: YieldSortBy::Return,
            filter_query: String::new(),
            is_filtering: false,
            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            is_search_loading: false,
            last_searched_query: String::new(),
        }
    }

    pub fn sort_opportunities(&mut self) {
        match self.sort_by {
            YieldSortBy::Return => {
                self.opportunities
                    .sort_by(|a, b| b.est_return.partial_cmp(&a.est_return).unwrap());
            },
            YieldSortBy::Volume => {
                self.opportunities
                    .sort_by(|a, b| b.volume.partial_cmp(&a.volume).unwrap());
            },
            YieldSortBy::EndDate => {
                self.opportunities
                    .sort_by(|a, b| match (&a.end_date, &b.end_date) {
                        (Some(a_date), Some(b_date)) => a_date.cmp(b_date),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    });
            },
        }
    }

    pub fn move_up(&mut self) {
        let filtered_len = self.filtered_opportunities().len();
        if filtered_len == 0 {
            return;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll {
                self.scroll = self.selected_index;
            }
        }
    }

    pub fn move_down(&mut self, visible_height: usize) {
        let filtered_len = self.filtered_opportunities().len();
        if filtered_len == 0 {
            return;
        }
        if self.selected_index < filtered_len.saturating_sub(1) {
            self.selected_index += 1;
            if self.selected_index >= self.scroll + visible_height {
                self.scroll = self.selected_index - visible_height + 1;
            }
        }
    }

    pub fn selected_opportunity(&self) -> Option<&YieldOpportunity> {
        self.filtered_opportunities()
            .get(self.selected_index)
            .copied()
    }

    /// Get filtered opportunities based on the current filter query
    pub fn filtered_opportunities(&self) -> Vec<&YieldOpportunity> {
        if self.filter_query.is_empty() {
            return self.opportunities.iter().collect();
        }

        let query_lower = self.filter_query.to_lowercase();
        self.opportunities
            .iter()
            .filter(|opp| {
                opp.event_title.to_lowercase().contains(&query_lower)
                    || opp.event_slug.to_lowercase().contains(&query_lower)
                    || opp.market_name.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    pub fn enter_filter_mode(&mut self) {
        self.is_filtering = true;
        self.filter_query.clear();
    }

    pub fn exit_filter_mode(&mut self) {
        self.is_filtering = false;
        // Keep the filter query so results stay filtered
    }

    #[allow(dead_code)]
    pub fn clear_filter(&mut self) {
        self.filter_query.clear();
        self.selected_index = 0;
        self.scroll = 0;
    }

    pub fn add_filter_char(&mut self, c: char) {
        self.filter_query.push(c);
        self.selected_index = 0;
        self.scroll = 0;
    }

    pub fn delete_filter_char(&mut self) {
        self.filter_query.pop();
        self.selected_index = 0;
        self.scroll = 0;
    }

    // API search methods
    pub fn enter_search_mode(&mut self) {
        self.is_searching = true;
        self.search_query.clear();
        self.search_results.clear();
        self.last_searched_query.clear();
    }

    pub fn exit_search_mode(&mut self) {
        self.is_searching = false;
        self.search_query.clear();
        self.search_results.clear();
        self.last_searched_query.clear();
        self.selected_index = 0;
        self.scroll = 0;
    }

    pub fn add_search_char(&mut self, c: char) {
        self.search_query.push(c);
        self.selected_index = 0;
        self.scroll = 0;
    }

    pub fn delete_search_char(&mut self) {
        self.search_query.pop();
        self.selected_index = 0;
        self.scroll = 0;
    }

    pub fn set_search_results(&mut self, results: Vec<YieldSearchResult>, query: String) {
        self.search_results = results;
        self.last_searched_query = query;
        self.is_search_loading = false;
        self.selected_index = 0;
        self.scroll = 0;
    }

    /// Check if we're in any input mode (filter or search)
    #[allow(dead_code)]
    pub fn is_in_input_mode(&self) -> bool {
        self.is_filtering || self.is_searching
    }

    /// Get the currently displayed items count
    #[allow(dead_code)]
    pub fn displayed_count(&self) -> usize {
        if self.is_searching && !self.search_results.is_empty() {
            self.search_results.len()
        } else {
            self.filtered_opportunities().len()
        }
    }

    /// Get selected search result (when in search mode)
    pub fn selected_search_result(&self) -> Option<&YieldSearchResult> {
        if self.is_searching || !self.search_results.is_empty() {
            self.search_results.get(self.selected_index)
        } else {
            None
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
    pub event_trade_counts: HashMap<String, usize>, // event_slug -> total trade count from API
    pub has_clob_auth: bool,       // Whether CLOB API authentication is available
    pub popup: Option<PopupType>,  // Currently active popup/modal
    pub trades_table_state: TableState, // State for trades table selection
    pub events_cache: HashMap<EventFilter, Vec<Event>>, // Cache for each filter tab
    pub show_logs: bool,           // Whether to show the logs panel (toggle with 'l')
    pub main_tab: MainTab,         // Current main tab (Trending vs Yield)
    pub yield_state: YieldState,   // State for the Yield tab
    pub auth_state: AuthState,     // Authentication state
    pub login_form: LoginFormState, // Login form state
}

impl TrendingAppState {
    pub fn new(events: Vec<Event>, order_by: String, ascending: bool, has_clob_auth: bool) -> Self {
        let current_limit = events.len();
        // Determine initial filter based on order_by
        let event_filter = if order_by == "startDate" || order_by == "startTime" {
            EventFilter::Breaking
        } else if order_by == "createdAt" || order_by == "creationTime" {
            EventFilter::New
        } else {
            EventFilter::Trending
        };
        // Initialize cache with the initial events for the current filter
        let mut events_cache = HashMap::new();
        events_cache.insert(event_filter, events.clone());
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
            event_trade_counts: HashMap::new(),
            has_clob_auth,
            popup: None,
            trades_table_state: TableState::default(),
            events_cache,
            show_logs: false, // Hidden by default
            main_tab: MainTab::Trending,
            yield_state: YieldState::new(),
            auth_state: AuthState::new(),
            login_form: LoginFormState::new(),
        }
    }

    /// Show a popup
    pub fn show_popup(&mut self, popup: PopupType) {
        self.popup = Some(popup);
    }

    /// Close the active popup
    pub fn close_popup(&mut self) {
        self.popup = None;
    }

    /// Check if a popup is active
    pub fn has_popup(&self) -> bool {
        self.popup.is_some()
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

    #[allow(dead_code)]
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
            // Reset markets scroll when changing events
            self.scroll.markets = 0;
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
            // Reset markets scroll when changing events
            self.scroll.markets = 0;
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
