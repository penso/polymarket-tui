//! State types for the trending TUI

use {
    chrono::{DateTime, Utc},
    polymarket_api::{gamma::Event, rtds::RTDSMessage},
    ratatui::widgets::TableState,
    std::collections::{HashMap, HashSet},
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

/// Main tab at the top level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MainTab {
    Trending,
    Favorites,
    Yield,
}

impl MainTab {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            MainTab::Trending => "Trending",
            MainTab::Favorites => "Favorites",
            MainTab::Yield => "Yield",
        }
    }

    #[allow(dead_code)]
    pub fn next(&self) -> Self {
        match self {
            MainTab::Trending => MainTab::Favorites,
            MainTab::Favorites => MainTab::Yield,
            MainTab::Yield => MainTab::Trending,
        }
    }

    #[allow(dead_code)]
    pub fn prev(&self) -> Self {
        match self {
            MainTab::Trending => MainTab::Yield,
            MainTab::Favorites => MainTab::Trending,
            MainTab::Yield => MainTab::Favorites,
        }
    }
}

/// Event filter type for different views
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventFilter {
    Trending, // Order by volume24hr (default)
    Breaking, // Order by oneDayPriceChange (biggest movers)
}

impl EventFilter {
    pub fn order_by(&self) -> &'static str {
        match self {
            EventFilter::Trending => "volume24hr",
            EventFilter::Breaking => "oneDayPriceChange",
        }
    }

    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            EventFilter::Trending => "Events",
            EventFilter::Breaking => "Breaking",
        }
    }

    #[allow(dead_code)]
    pub fn next(&self) -> Self {
        match self {
            EventFilter::Trending => EventFilter::Breaking,
            EventFilter::Breaking => EventFilter::Trending,
        }
    }

    #[allow(dead_code)]
    pub fn prev(&self) -> Self {
        match self {
            EventFilter::Trending => EventFilter::Breaking,
            EventFilter::Breaking => EventFilter::Trending,
        }
    }
}

/// Sort options for events list (matches Polymarket website options)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventSortBy {
    #[default]
    Volume24hr, // 24h Volume (default for Trending)
    VolumeTotal, // Total Volume
    Liquidity,   // Liquidity
    Newest,      // Newest (by created date)
    EndingSoon,  // Ending Soon
    Competitive, // Competitive (closer odds)
}

impl EventSortBy {
    pub fn label(&self) -> &'static str {
        match self {
            EventSortBy::Volume24hr => "24h Vol",
            EventSortBy::VolumeTotal => "Total Vol",
            EventSortBy::Liquidity => "Liquidity",
            EventSortBy::Newest => "Newest",
            EventSortBy::EndingSoon => "Ending Soon",
            EventSortBy::Competitive => "Competitive",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            EventSortBy::Volume24hr => EventSortBy::VolumeTotal,
            EventSortBy::VolumeTotal => EventSortBy::Liquidity,
            EventSortBy::Liquidity => EventSortBy::Newest,
            EventSortBy::Newest => EventSortBy::EndingSoon,
            EventSortBy::EndingSoon => EventSortBy::Competitive,
            EventSortBy::Competitive => EventSortBy::Volume24hr,
        }
    }

    /// Get the API order parameter for this sort option
    #[allow(dead_code)]
    pub fn api_order_param(&self) -> &'static str {
        match self {
            EventSortBy::Volume24hr => "volume24hr",
            EventSortBy::VolumeTotal => "volume",
            EventSortBy::Liquidity => "liquidity",
            EventSortBy::Newest => "createdAt",
            EventSortBy::EndingSoon => "endDate",
            EventSortBy::Competitive => "competitive",
        }
    }

    /// Whether this sort should be ascending (true) or descending (false)
    #[allow(dead_code)]
    pub fn is_ascending(&self) -> bool {
        match self {
            EventSortBy::EndingSoon => true, // Soonest first
            EventSortBy::Newest => false,    // Most recent first
            _ => false,                      // Highest values first
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
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum PopupType {
    Help,              // Show help/keyboard shortcuts
    ConfirmQuit,       // Confirm before quitting
    EventInfo(String), // Show detailed event info (slug)
    Login,             // Login modal with credential input
    UserProfile,       // Show authenticated user profile
    Trade,             // Trade modal (form state is in app.trade_form)
}

/// Login form field being edited
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    ApiKey,
    Secret,
    Passphrase,
    Address,
    // Optional cookie fields for favorites
    SessionCookie,
    SessionNonce,
    SessionAuthType,
}

/// Trade side (Buy or Sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSide {
    Buy,
    Sell,
}

impl TradeSide {
    pub fn toggle(&self) -> Self {
        match self {
            TradeSide::Buy => TradeSide::Sell,
            TradeSide::Sell => TradeSide::Buy,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TradeSide::Buy => "BUY",
            TradeSide::Sell => "SELL",
        }
    }
}

/// Trade form field being edited
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeField {
    Side,
    Amount,
}

impl TradeField {
    pub fn next(&self) -> Self {
        match self {
            TradeField::Side => TradeField::Amount,
            TradeField::Amount => TradeField::Side,
        }
    }

    pub fn prev(&self) -> Self {
        self.next() // Only 2 fields
    }
}

/// Trade form state
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TradeFormState {
    pub token_id: String,
    pub market_question: String,
    pub outcome: String,
    pub side: TradeSide,
    pub amount: String, // Amount in dollars (input as string for editing)
    pub price: f64,     // Current market price
    pub active_field: TradeField,
    pub error_message: Option<String>,
    pub is_submitting: bool,
}

impl TradeFormState {
    pub fn new(token_id: String, market_question: String, outcome: String, price: f64) -> Self {
        Self {
            token_id,
            market_question,
            outcome,
            side: TradeSide::Buy,
            amount: String::new(),
            price,
            active_field: TradeField::Amount,
            error_message: None,
            is_submitting: false,
        }
    }

    pub fn add_char(&mut self, c: char) {
        if self.active_field == TradeField::Amount {
            // Only allow numeric input and decimal point
            if c.is_ascii_digit() || (c == '.' && !self.amount.contains('.')) {
                self.amount.push(c);
            }
        }
        self.error_message = None;
    }

    pub fn delete_char(&mut self) {
        if self.active_field == TradeField::Amount {
            self.amount.pop();
        }
        self.error_message = None;
    }

    pub fn toggle_side(&mut self) {
        self.side = self.side.toggle();
        self.error_message = None;
    }

    pub fn amount_f64(&self) -> f64 {
        self.amount.parse().unwrap_or(0.0)
    }

    /// Calculate estimated shares based on amount and price
    pub fn estimated_shares(&self) -> f64 {
        let amount = self.amount_f64();
        if self.price > 0.0 {
            amount / self.price
        } else {
            0.0
        }
    }

    /// Calculate potential profit (for buy: payout - cost, for sell: proceeds)
    pub fn potential_profit(&self) -> f64 {
        let shares = self.estimated_shares();
        match self.side {
            TradeSide::Buy => shares - self.amount_f64(), // Shares pay $1 each if won
            TradeSide::Sell => self.amount_f64(),         // Proceeds from selling
        }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.amount.clear();
        self.side = TradeSide::Buy;
        self.active_field = TradeField::Amount;
        self.error_message = None;
        self.is_submitting = false;
    }
}

#[allow(dead_code)]
impl LoginField {
    pub fn next(&self) -> Self {
        match self {
            LoginField::ApiKey => LoginField::Secret,
            LoginField::Secret => LoginField::Passphrase,
            LoginField::Passphrase => LoginField::Address,
            LoginField::Address => LoginField::SessionCookie,
            LoginField::SessionCookie => LoginField::SessionNonce,
            LoginField::SessionNonce => LoginField::SessionAuthType,
            LoginField::SessionAuthType => LoginField::ApiKey,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            LoginField::ApiKey => LoginField::SessionAuthType,
            LoginField::Secret => LoginField::ApiKey,
            LoginField::Passphrase => LoginField::Secret,
            LoginField::Address => LoginField::Passphrase,
            LoginField::SessionCookie => LoginField::Address,
            LoginField::SessionNonce => LoginField::SessionCookie,
            LoginField::SessionAuthType => LoginField::SessionNonce,
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
    // Optional cookie fields for favorites functionality
    pub session_cookie: String,
    pub session_nonce: String,
    pub session_auth_type: String,
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
            session_cookie: String::new(),
            session_nonce: String::new(),
            session_auth_type: String::from("magic"), // Default to "magic"
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
            LoginField::SessionCookie => &self.session_cookie,
            LoginField::SessionNonce => &self.session_nonce,
            LoginField::SessionAuthType => &self.session_auth_type,
        }
    }

    pub fn add_char(&mut self, c: char) {
        match self.active_field {
            LoginField::ApiKey => self.api_key.push(c),
            LoginField::Secret => self.secret.push(c),
            LoginField::Passphrase => self.passphrase.push(c),
            LoginField::Address => self.address.push(c),
            LoginField::SessionCookie => self.session_cookie.push(c),
            LoginField::SessionNonce => self.session_nonce.push(c),
            LoginField::SessionAuthType => self.session_auth_type.push(c),
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
            LoginField::SessionCookie => {
                self.session_cookie.pop();
            },
            LoginField::SessionNonce => {
                self.session_nonce.pop();
            },
            LoginField::SessionAuthType => {
                self.session_auth_type.pop();
            },
        }
        self.error_message = None;
    }

    pub fn clear(&mut self) {
        self.api_key.clear();
        self.secret.clear();
        self.passphrase.clear();
        self.address.clear();
        self.session_cookie.clear();
        self.session_nonce.clear();
        self.session_auth_type = String::from("magic"); // Reset to default
        self.active_field = LoginField::ApiKey;
        self.error_message = None;
        self.is_validating = false;
    }
}

/// User profile information from Polymarket
#[derive(Debug, Clone, Default)]
pub struct UserProfile {
    pub name: Option<String>,
    pub pseudonym: Option<String>,
    pub bio: Option<String>,
    pub profile_image: Option<String>,
}

/// User authentication state
#[derive(Debug, Clone)]
pub struct AuthState {
    pub is_authenticated: bool,
    pub username: Option<String>,
    pub address: Option<String>,
    pub balance: Option<f64>,           // USDC cash balance
    pub portfolio_value: Option<f64>,   // Total portfolio value (positions)
    pub positions_count: Option<usize>, // Number of open positions
    pub unrealized_pnl: Option<f64>,    // Unrealized profit/loss
    pub realized_pnl: Option<f64>,      // Realized profit/loss
    pub profile: Option<UserProfile>,
}

impl AuthState {
    pub fn new() -> Self {
        Self {
            is_authenticated: false,
            username: None,
            address: None,
            balance: None,
            portfolio_value: None,
            positions_count: None,
            unrealized_pnl: None,
            realized_pnl: None,
            profile: None,
        }
    }

    pub fn display_name(&self) -> String {
        if let Some(ref name) = self.username {
            name.clone()
        } else if let Some(ref addr) = self.address {
            addr.clone()
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
/// Full event details are looked up from the global event_cache using event_slug
/// Some event data is cached here for filtering and sorting purposes
#[derive(Debug, Clone)]
pub struct YieldOpportunity {
    pub market_name: String,
    pub market_status: &'static str,
    pub outcome: String,
    pub price: f64,
    pub est_return: f64,
    pub volume: f64,
    pub event_slug: String,
    // Cached event data for filtering/sorting (full details from event_cache)
    pub event_title: String,
    pub end_date: Option<DateTime<Utc>>,
}

/// A search result in the Yield tab - an event with its best yield opportunity (if any)
/// Event details are looked up from the global event_cache using event_slug
#[derive(Debug, Clone)]
pub struct YieldSearchResult {
    pub event_slug: String,
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

    /// Hide the search input but keep the search results displayed
    pub fn hide_search_input(&mut self) {
        self.is_searching = false;
        // Keep search_query, search_results, and last_searched_query intact
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

/// Favorites tab state
#[derive(Debug)]
pub struct FavoritesState {
    pub events: Vec<Event>,
    pub favorite_ids: Vec<polymarket_api::FavoriteEvent>, // Favorite entries from API
    pub favorite_event_slugs: HashSet<String>,            // Quick lookup for favorite slugs
    pub selected_index: usize,
    pub scroll: usize,
    pub is_loading: bool,
    pub error_message: Option<String>,
}

#[allow(dead_code)]
impl FavoritesState {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            favorite_ids: Vec::new(),
            favorite_event_slugs: HashSet::new(),
            selected_index: 0,
            scroll: 0,
            is_loading: false,
            error_message: None,
        }
    }

    pub fn selected_event(&self) -> Option<&Event> {
        self.events.get(self.selected_index)
    }

    /// Check if an event slug is in favorites
    pub fn is_favorite(&self, slug: &str) -> bool {
        self.favorite_event_slugs.contains(slug)
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll {
                self.scroll = self.selected_index;
            }
        }
    }

    pub fn move_down(&mut self, visible_height: usize) {
        if self.selected_index + 1 < self.events.len() {
            self.selected_index += 1;
            if self.selected_index >= self.scroll + visible_height {
                self.scroll = self.selected_index - visible_height + 1;
            }
        }
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.favorite_ids.clear();
        self.selected_index = 0;
        self.scroll = 0;
        self.error_message = None;
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
    pub event_filter: EventFilter, // Current filter (Trending, Breaking)
    pub market_prices: HashMap<String, f64>, // asset_id -> current price from API
    pub event_trade_counts: HashMap<String, usize>, // event_slug -> total trade count from API
    pub has_clob_auth: bool,       // Whether CLOB API authentication is available
    pub popup: Option<PopupType>,  // Currently active popup/modal
    pub trades_table_state: TableState, // State for trades table selection
    pub events_cache: HashMap<EventFilter, Vec<Event>>, // Cache for each filter tab
    /// Global event cache keyed by slug - single source of truth for event data
    pub event_cache: HashMap<String, Event>,
    pub show_logs: bool,   // Whether to show the logs panel (toggle with 'l')
    pub main_tab: MainTab, // Current main tab (Trending vs Yield)
    pub yield_state: YieldState, // State for the Yield tab
    pub favorites_state: FavoritesState, // State for the Favorites tab
    pub auth_state: AuthState, // Authentication state
    pub login_form: LoginFormState, // Login form state
    pub trade_form: Option<TradeFormState>, // Trade form state (when trade popup is open)
    pub event_sort_by: EventSortBy, // Current sort option for events list
    pub api_status: Option<bool>, // API health status: Some(true) = healthy, Some(false) = unhealthy, None = unknown
}

impl TrendingAppState {
    pub fn new(events: Vec<Event>, order_by: String, ascending: bool, has_clob_auth: bool) -> Self {
        let current_limit = events.len();
        // Determine initial filter based on order_by
        let event_filter = if order_by == "startDate"
            || order_by == "startTime"
            || order_by == "oneDayPriceChange"
        {
            EventFilter::Breaking
        } else {
            EventFilter::Trending
        };
        // Initialize cache with the initial events for the current filter
        let mut events_cache = HashMap::new();
        events_cache.insert(event_filter, events.clone());
        // Initialize global event cache with initial events
        let mut event_cache = HashMap::new();
        for event in &events {
            event_cache.insert(event.slug.clone(), event.clone());
        }
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
            event_cache,
            show_logs: false, // Hidden by default
            main_tab: MainTab::Trending,
            yield_state: YieldState::new(),
            favorites_state: FavoritesState::new(),
            auth_state: AuthState::new(),
            login_form: LoginFormState::new(),
            trade_form: None,
            event_sort_by: EventSortBy::default(),
            api_status: None,
        }
    }

    /// Add events to the global cache
    pub fn cache_events(&mut self, events: &[Event]) {
        for event in events {
            self.event_cache.insert(event.slug.clone(), event.clone());
        }
    }

    /// Get an event from the global cache by slug
    pub fn get_cached_event(&self, slug: &str) -> Option<&Event> {
        self.event_cache.get(slug)
    }

    /// Sort events by the current sort option
    pub fn sort_events(&mut self) {
        match self.event_sort_by {
            EventSortBy::Volume24hr => {
                self.events.sort_by(|a, b| {
                    b.volume_24hr
                        .partial_cmp(&a.volume_24hr)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            },
            EventSortBy::VolumeTotal => {
                self.events.sort_by(|a, b| {
                    b.volume
                        .partial_cmp(&a.volume)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            },
            EventSortBy::Liquidity => {
                self.events.sort_by(|a, b| {
                    b.liquidity
                        .partial_cmp(&a.liquidity)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            },
            EventSortBy::Newest => {
                // Sort by created_at descending (newest first)
                self.events
                    .sort_by(|a, b| match (&b.created_at, &a.created_at) {
                        (Some(b_date), Some(a_date)) => b_date.cmp(a_date),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    });
            },
            EventSortBy::EndingSoon => {
                // Sort by end_date ascending (soonest first), None at end
                self.events
                    .sort_by(|a, b| match (&a.end_date, &b.end_date) {
                        (Some(a_date), Some(b_date)) => a_date.cmp(b_date),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    });
            },
            EventSortBy::Competitive => {
                // Sort by competitive score descending (most competitive first)
                self.events.sort_by(|a, b| {
                    b.competitive
                        .partial_cmp(&a.competitive)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            },
        }
    }

    /// Show a popup
    pub fn show_popup(&mut self, popup: PopupType) {
        self.popup = Some(popup);
    }

    /// Close the active popup
    pub fn close_popup(&mut self) {
        self.popup = None;
        // Clear trade form when closing trade popup
        self.trade_form = None;
    }

    /// Open trade popup for a specific market
    pub fn open_trade_popup(
        &mut self,
        token_id: String,
        market_question: String,
        outcome: String,
        price: f64,
    ) {
        self.trade_form = Some(TradeFormState::new(
            token_id,
            market_question,
            outcome,
            price,
        ));
        self.popup = Some(PopupType::Trade);
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
    /// For Favorites tab, returns favorites events (search not supported yet)
    pub fn filtered_events(&self) -> Vec<&Event> {
        // For Favorites tab, just return favorites events (no search support yet)
        if self.main_tab == MainTab::Favorites {
            return self.favorites_state.events.iter().collect();
        }

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
    /// Uses current_selected_index() to be tab-aware
    pub fn selected_event_filtered(&self) -> Option<&Event> {
        let filtered = self.filtered_events();
        let selected_idx = self.current_selected_index();
        filtered.get(selected_idx).copied()
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
        self.search.results.clear();
        self.search.last_searched_query.clear();
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

    /// Get the current tab's selected index
    pub fn current_selected_index(&self) -> usize {
        match self.main_tab {
            MainTab::Favorites => self.favorites_state.selected_index,
            _ => self.navigation.selected_index,
        }
    }

    /// Get the current tab's scroll position for the events list
    pub fn current_events_scroll(&self) -> usize {
        match self.main_tab {
            MainTab::Favorites => self.favorites_state.scroll,
            _ => self.scroll.events_list,
        }
    }

    /// Get events for the current tab (without filtering)
    #[allow(dead_code)]
    pub fn current_events(&self) -> Vec<&Event> {
        match self.main_tab {
            MainTab::Favorites => self.favorites_state.events.iter().collect(),
            _ => self.events.iter().collect(),
        }
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
