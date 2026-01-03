//! TUI for browsing trending events with live trade monitoring

mod keys;
mod render;
mod state;

use {
    chrono::{DateTime, Utc},
    ratatui::layout::{Constraint, Direction, Layout, Rect},
    render::{ClickedTab, render, truncate},
    state::{
        EventFilter, EventTrades, FocusedPanel, MainTab, PopupType, SearchMode, YieldOpportunity,
        YieldSearchResult,
    },
};

pub use state::TrendingAppState;

/// Macros for conditional logging based on tracing feature
#[cfg(feature = "tracing")]
macro_rules! log_info {
    ($($arg:tt)*) => { tracing::info!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_info {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! log_debug {
    ($($arg:tt)*) => { tracing::debug!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_debug {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! log_error {
    ($($arg:tt)*) => { tracing::error!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_error {
    ($($arg:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! log_warn {
    ($($arg:tt)*) => { tracing::warn!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_warn {
    ($($arg:tt)*) => {};
}

use {
    polymarket_api::{
        GammaClient,
        clob::{BatchTokenRequest, ClobClient, Side},
    },
    ratatui::{Terminal, backend::CrosstermBackend},
    std::{collections::HashMap, io, sync::Arc},
    tokio::sync::Mutex as TokioMutex,
};

/// Helper to calculate panel areas for mouse click detection
/// Returns (header_area, events_list_area, event_details_area, markets_area, trades_area, logs_area)
fn calculate_panel_areas(
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
fn get_panel_at_position(
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

/// Switch to a new filter tab, using cache if available.
/// Returns `Some((order_by, limit))` if API fetch is needed, `None` if cache was used.
fn switch_filter_tab(
    app: &mut TrendingAppState,
    new_filter: EventFilter,
) -> Option<(EventFilter, usize)> {
    if new_filter == app.event_filter {
        return None;
    }

    app.event_filter = new_filter;
    // Clear all search state when switching tabs
    app.search.results.clear();
    app.search.last_searched_query.clear();
    app.search.query.clear();
    app.search.mode = SearchMode::None;
    app.search.is_searching = false;
    app.navigation.selected_index = 0;
    app.scroll.events_list = 0;
    app.pagination.order_by = new_filter.order_by().to_string();
    app.pagination.ascending = false;

    // Check cache first
    if let Some(cached_events) = app.events_cache.get(&new_filter) {
        log_info!(
            "Using cached {} events for {} filter",
            cached_events.len(),
            new_filter.label()
        );
        app.events = cached_events.clone();
        None
    } else {
        // Need to fetch from API - clear events to show loading state
        app.events.clear();
        app.pagination.is_fetching_more = true;
        log_info!(
            "Switching to {} filter, fetching events...",
            new_filter.label()
        );
        Some((new_filter, app.pagination.current_limit))
    }
}

/// Spawn async task to fetch events for a filter tab
fn spawn_filter_fetch(
    app_state: Arc<TokioMutex<TrendingAppState>>,
    filter: EventFilter,
    limit: usize,
) {
    let gamma_client = GammaClient::new();

    tokio::spawn(async move {
        match fetch_events_for_filter(&gamma_client, filter, limit).await {
            Ok(new_events) => {
                log_info!(
                    "Fetched {} events for {:?} filter",
                    new_events.len(),
                    filter
                );
                let mut app = app_state.lock().await;
                // Cache events in global event cache
                app.cache_events(&new_events);
                app.events_cache.insert(filter, new_events.clone());
                app.events = new_events;
                app.pagination.is_fetching_more = false;
                app.navigation.selected_index = 0;
                app.scroll.events_list = 0;
            },
            Err(e) => {
                log_error!("Failed to fetch events: {}", e);
                let mut app = app_state.lock().await;
                app.pagination.is_fetching_more = false;
            },
        }
    });
}

/// Fetch events for a given filter using the appropriate API call
async fn fetch_events_for_filter(
    gamma_client: &GammaClient,
    filter: EventFilter,
    limit: usize,
) -> crate::Result<Vec<polymarket_api::gamma::Event>> {
    match filter {
        EventFilter::Breaking => {
            // Breaking = markets that moved the most in the last 24 hours
            gamma_client
                .get_breaking_events(Some(limit))
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        },
        _ => {
            // Trending and New use the events endpoint with different ordering
            gamma_client
                .get_trending_events(Some(filter.order_by()), Some(false), Some(limit))
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        },
    }
}

/// Spawn async task to fetch user profile by address and update auth state
fn spawn_fetch_user_profile(app_state: Arc<TokioMutex<TrendingAppState>>, address: String) {
    let gamma_client = GammaClient::new();

    tokio::spawn(async move {
        match gamma_client.get_public_profile(&address).await {
            Ok(Some(profile)) => {
                // Prefer name, fall back to pseudonym for display
                let username = profile
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .or_else(|| profile.pseudonym.clone().filter(|p| !p.is_empty()));

                log_info!(
                    "Fetched profile for {}: {:?}",
                    address,
                    username.as_deref().unwrap_or("(no name)")
                );

                let mut app = app_state.lock().await;
                app.auth_state.username = username.clone();

                // Store full profile info
                app.auth_state.profile = Some(state::UserProfile {
                    name: profile.name,
                    pseudonym: profile.pseudonym,
                    bio: profile.bio,
                    profile_image: profile.profile_image,
                });

                // Also update saved auth config with username
                if let Some(mut config) = crate::auth::AuthConfig::load() {
                    config.username = username;
                    let _ = config.save();
                }
            },
            Ok(None) => {
                log_debug!("No profile found for address {}", address);
            },
            Err(e) => {
                log_debug!("Failed to fetch profile for {}: {}", address, e);
            },
        }
    });
}

/// Spawn async task to fetch user's portfolio data (balance, positions)
fn spawn_fetch_portfolio(app_state: Arc<TokioMutex<TrendingAppState>>, address: String) {
    use polymarket_api::{DataClient, clob::AssetType};

    tokio::spawn(async move {
        let clob_client = ClobClient::from_env();
        let data_client = DataClient::new();

        // Fetch USDC balance
        if clob_client.has_auth() {
            match clob_client
                .get_balance_allowance(AssetType::Collateral)
                .await
            {
                Ok(balance_info) => {
                    // Balance is in smallest units (6 decimals for USDC)
                    let balance: f64 = balance_info
                        .balance
                        .parse()
                        .map(|b: f64| b / 1_000_000.0)
                        .unwrap_or(0.0);
                    log_info!("Fetched balance: ${:.2} USDC", balance);

                    let mut app = app_state.lock().await;
                    app.auth_state.balance = Some(balance);
                },
                Err(e) => {
                    log_debug!("Failed to fetch balance: {}", e);
                },
            }
        }

        // Fetch positions
        match data_client.get_positions(&address).await {
            Ok(positions) => {
                // Calculate totals from positions
                let total_value: f64 = positions.iter().filter_map(|p| p.current_value).sum();
                let positions_count = positions.len();

                // Sum up unrealized P&L (cash_pnl) from all positions
                let unrealized_pnl: f64 = positions.iter().filter_map(|p| p.cash_pnl).sum();

                // Sum up realized P&L from all positions
                let realized_pnl: f64 = positions.iter().filter_map(|p| p.realized_pnl).sum();

                log_info!(
                    "Fetched portfolio: {} positions, ${:.2} value, unrealized P&L: ${:.2}, realized P&L: ${:.2}",
                    positions_count,
                    total_value,
                    unrealized_pnl,
                    realized_pnl
                );

                let mut app = app_state.lock().await;
                app.auth_state.portfolio_value = Some(total_value);
                app.auth_state.positions_count = Some(positions_count);
                app.auth_state.unrealized_pnl = Some(unrealized_pnl);
                app.auth_state.realized_pnl = Some(realized_pnl);
            },
            Err(e) => {
                log_debug!("Failed to fetch positions: {}", e);
            },
        }
    });
}

/// Spawn async task to toggle favorite status for an event
fn spawn_toggle_favorite(
    app_state: Arc<TokioMutex<TrendingAppState>>,
    event_id: String,
    event_slug: String,
    event: Option<polymarket_api::gamma::Event>,
) {
    use polymarket_api::{GammaAuth, GammaClient};

    tokio::spawn(async move {
        // Load auth config to get session cookies
        let auth_config = match crate::auth::AuthConfig::load() {
            Some(config) => config,
            None => {
                log_error!("No auth config found for toggling favorite");
                return;
            },
        };

        // Check if session cookie is available
        if auth_config.session_cookie.is_none() {
            log_error!("Session cookie required for favorites");
            return;
        }

        // Create authenticated gamma client with session cookies
        let gamma_auth = GammaAuth {
            api_key: auth_config.api_key,
            api_secret: auth_config.secret,
            passphrase: auth_config.passphrase,
            address: auth_config.address,
            session_cookie: auth_config.session_cookie,
            session_nonce: auth_config.session_nonce,
            session_auth_type: auth_config.session_auth_type,
        };
        let gamma_client = GammaClient::with_auth(gamma_auth);

        // Check current favorite status and toggle
        let is_currently_favorite = {
            let app = app_state.lock().await;
            app.favorites_state.is_favorite(&event_slug)
        };

        if is_currently_favorite {
            // Find the favorite ID to remove
            let favorite_id = {
                let app = app_state.lock().await;
                app.favorites_state
                    .favorite_ids
                    .iter()
                    .find(|f| f.event_id == event_id)
                    .map(|f| f.id)
            };

            if let Some(fav_id) = favorite_id {
                match gamma_client.remove_favorite_event(fav_id).await {
                    Ok(()) => {
                        log_info!("Removed favorite: {}", event_slug);
                        let mut app = app_state.lock().await;
                        app.favorites_state.favorite_event_slugs.remove(&event_slug);
                        app.favorites_state
                            .favorite_ids
                            .retain(|f| f.event_id != event_id);
                        app.favorites_state.events.retain(|e| e.slug != event_slug);
                    },
                    Err(e) => {
                        log_error!("Failed to remove favorite: {}", e);
                    },
                }
            }
        } else {
            // Add to favorites
            match gamma_client.add_favorite_event(&event_id).await {
                Ok(favorite_entry) => {
                    log_info!("Added favorite: {}", event_slug);
                    let mut app = app_state.lock().await;
                    app.favorites_state
                        .favorite_event_slugs
                        .insert(event_slug.clone());
                    app.favorites_state.favorite_ids.push(favorite_entry);
                    // Add the event to favorites list if we have the full event data
                    if let Some(evt) = event {
                        app.favorites_state.events.push(evt);
                    }
                },
                Err(e) => {
                    log_error!("Failed to add favorite: {}", e);
                },
            }
        }
    });
}

/// Spawn async task to fetch event by slug and then toggle favorite
/// Used when event is not in cache (e.g., from Yield tab)
fn spawn_fetch_and_toggle_favorite(
    app_state: Arc<TokioMutex<TrendingAppState>>,
    event_slug: String,
) {
    use polymarket_api::GammaClient;

    tokio::spawn(async move {
        let gamma_client = GammaClient::new();

        // Fetch the event by slug to get its ID
        match gamma_client.get_event_by_slug(&event_slug).await {
            Ok(Some(event)) => {
                log_info!("Fetched event for bookmark: {}", event.slug);
                spawn_toggle_favorite(app_state, event.id.clone(), event.slug.clone(), Some(event));
            },
            Ok(None) => {
                log_error!("Event not found: {}", event_slug);
            },
            Err(e) => {
                log_error!("Failed to fetch event {}: {}", event_slug, e);
            },
        }
    });
}

/// Fetch trade count for an event's markets using authenticated CLOB API
/// Returns total number of trades across all markets in the event
async fn fetch_event_trade_count(
    clob_client: &ClobClient,
    condition_ids: Vec<String>,
) -> Option<usize> {
    if !clob_client.has_auth() {
        return None;
    }

    let mut total_count = 0;
    for condition_id in condition_ids {
        match clob_client
            .get_trades_authenticated(&condition_id, Some(1000))
            .await
        {
            Ok(trades) => {
                total_count += trades.len();
            },
            Err(_e) => {
                log_debug!("Failed to fetch trades for market {}: {}", condition_id, _e);
                // Continue with other markets even if one fails
            },
        }
    }
    Some(total_count)
}

/// Fetch market prices using the batch API
/// Returns a HashMap mapping asset_id to the best ask price
async fn fetch_market_prices_batch(
    clob_client: &ClobClient,
    active_markets: Vec<Vec<String>>,
) -> HashMap<String, f64> {
    // Collect all asset IDs from active markets
    let all_asset_ids: Vec<String> = active_markets.into_iter().flatten().collect();

    if all_asset_ids.is_empty() {
        return HashMap::new();
    }

    // Build batch requests - we want SELL side to get ask prices
    let requests: Vec<BatchTokenRequest> = all_asset_ids
        .iter()
        .map(|asset_id| BatchTokenRequest {
            token_id: asset_id.clone(),
            side: Side::Sell,
        })
        .collect();

    let request_count = requests.len();
    log_debug!("Fetching {} market prices via batch API", request_count);

    // Try batch orderbooks first (more reliable for getting best ask)
    match clob_client.get_orderbooks(requests).await {
        Ok(orderbooks) => {
            let mut prices = HashMap::new();
            for orderbook in orderbooks {
                if let Some(asset_id) = &orderbook.asset_id {
                    // Find the best (lowest) ask price
                    let best_price = orderbook
                        .asks
                        .iter()
                        .filter_map(|ask| ask.price.parse::<f64>().ok())
                        .min_by(|a, b| a.partial_cmp(b).unwrap());
                    if let Some(price) = best_price {
                        prices.insert(asset_id.clone(), price);
                    }
                }
            }
            log_debug!(
                "Batch API returned prices for {} of {} assets",
                prices.len(),
                request_count
            );
            prices
        },
        Err(_e) => {
            log_debug!(
                "Batch orderbooks failed: {}, falling back to individual calls",
                _e
            );
            // Fallback to individual calls if batch fails
            let mut prices = HashMap::new();
            for asset_id in all_asset_ids {
                if let Ok(orderbook) = clob_client.get_orderbook_by_asset(&asset_id).await {
                    let best_price = orderbook
                        .asks
                        .iter()
                        .filter_map(|ask| ask.price.parse::<f64>().ok())
                        .min_by(|a, b| a.partial_cmp(b).unwrap());
                    if let Some(price) = best_price {
                        prices.insert(asset_id.clone(), price);
                    }
                }
            }
            prices
        },
    }
}

/// Fetch yield opportunities from the Gamma API
async fn fetch_yield_opportunities(
    min_prob: f64,
    limit: usize,
    min_volume: f64,
) -> Vec<YieldOpportunity> {
    let gamma_client = GammaClient::new();

    // Fetch active markets
    let markets = match gamma_client
        .get_markets(Some(true), Some(false), Some(limit))
        .await
    {
        Ok(m) => m,
        Err(e) => {
            log_error!("Failed to fetch markets for yield: {}", e);
            return Vec::new();
        },
    };

    log_info!("Fetched {} markets, filtering for yield...", markets.len());

    let mut opportunities: Vec<YieldOpportunity> = Vec::new();

    for market in &markets {
        // Skip closed markets
        if market.closed {
            continue;
        }

        // Skip markets without event info
        let event = match market.event() {
            Some(e) => e,
            None => continue,
        };

        // Parse end date
        let end_date = event
            .end_date
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        // Check volume threshold
        let volume = market.volume_24hr.unwrap_or(0.0);
        if volume < min_volume {
            continue;
        }

        // Check each outcome price
        for (i, price_str) in market.outcome_prices.iter().enumerate() {
            if let Ok(price) = price_str.parse::<f64>()
                && price >= min_prob
                && price < 1.0
            // Skip 100% price (no yield)
            {
                let outcome = market
                    .outcomes
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("Outcome {}", i));
                let est_return = (1.0 - price) * 100.0;

                // Use short name if available
                let market_name = market
                    .group_item_title
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| market.question.clone());

                opportunities.push(YieldOpportunity {
                    market_name,
                    market_status: market.status(),
                    outcome,
                    price,
                    est_return,
                    volume,
                    event_slug: event.slug.clone(),
                    event_title: event.title.clone(),
                    end_date,
                });
            }
        }
    }

    // Sort by estimated return (highest first)
    opportunities.sort_by(|a, b| b.est_return.partial_cmp(&a.est_return).unwrap());

    log_info!("Found {} yield opportunities", opportunities.len());
    opportunities
}

/// Spawn async task to fetch yield opportunities
fn spawn_yield_fetch(app_state: Arc<TokioMutex<TrendingAppState>>) {
    tokio::spawn(async move {
        let (min_prob, min_volume) = {
            let mut app = app_state.lock().await;
            app.yield_state.is_loading = true;
            (app.yield_state.min_prob, app.yield_state.min_volume)
        };

        log_info!(
            "Fetching yield opportunities (min_prob: {:.0}%)...",
            min_prob * 100.0
        );

        let opportunities = fetch_yield_opportunities(min_prob, 500, min_volume).await;

        let mut app = app_state.lock().await;
        app.yield_state.opportunities = opportunities;
        app.yield_state.is_loading = false;
        app.yield_state.selected_index = 0;
        app.yield_state.scroll = 0;
        app.yield_state.sort_opportunities();

        log_info!(
            "Loaded {} yield opportunities",
            app.yield_state.opportunities.len()
        );
    });
}

/// Spawn async task to fetch favorite events
fn spawn_fetch_favorites(app_state: Arc<TokioMutex<TrendingAppState>>) {
    use polymarket_api::{GammaAuth, GammaClient};

    tokio::spawn(async move {
        // Set loading state
        {
            let mut app = app_state.lock().await;
            app.favorites_state.is_loading = true;
            app.favorites_state.error_message = None;
        }

        // Load auth config
        let auth_config = match crate::auth::AuthConfig::load() {
            Some(config) => config,
            None => {
                let mut app = app_state.lock().await;
                app.favorites_state.is_loading = false;
                app.favorites_state.error_message = Some("No auth credentials found".to_string());
                return;
            },
        };

        // Check if session cookie is available
        if auth_config.session_cookie.is_none() {
            let mut app = app_state.lock().await;
            app.favorites_state.is_loading = false;
            app.favorites_state.error_message = Some(
                "Session cookie required. Add 'session_cookie' to your auth.json config \
                 with the value of 'polymarketsession' cookie from browser dev tools."
                    .to_string(),
            );
            return;
        }

        // Create authenticated gamma client with session cookies
        let gamma_auth = GammaAuth {
            api_key: auth_config.api_key,
            api_secret: auth_config.secret,
            passphrase: auth_config.passphrase,
            address: auth_config.address,
            session_cookie: auth_config.session_cookie,
            session_nonce: auth_config.session_nonce,
            session_auth_type: auth_config.session_auth_type,
        };
        let gamma_client = GammaClient::with_auth(gamma_auth);

        log_info!("Fetching favorite events...");

        // Fetch favorite event IDs
        let favorites = match gamma_client.get_favorite_events().await {
            Ok(favs) => favs,
            Err(e) => {
                log_error!("Failed to fetch favorites: {}", e);
                let mut app = app_state.lock().await;
                app.favorites_state.is_loading = false;
                app.favorites_state.error_message = Some(format!("Failed to fetch: {}", e));
                return;
            },
        };

        log_info!("Found {} favorites", favorites.len());

        // Fetch full event data for each favorite (the embedded events have empty markets)
        let mut events = Vec::with_capacity(favorites.len());
        for fav in &favorites {
            match gamma_client.get_event_by_id(&fav.event_id).await {
                Ok(Some(event)) => {
                    log_info!(
                        "Fetched event: {} with {} markets",
                        event.title,
                        event.markets.len()
                    );
                    events.push(event);
                },
                Ok(None) => {
                    log_warn!("Event {} not found", fav.event_id);
                },
                Err(e) => {
                    log_error!("Failed to fetch event {}: {}", fav.event_id, e);
                },
            }
        }

        log_info!("Loaded {} favorite events with full data", events.len());

        // Build slug lookup set for quick favorite checking
        let favorite_slugs: std::collections::HashSet<String> =
            events.iter().map(|e| e.slug.clone()).collect();

        // Update state
        let mut app = app_state.lock().await;
        // Cache events in global event cache
        app.cache_events(&events);
        app.favorites_state.events = events;
        app.favorites_state.favorite_ids = favorites;
        app.favorites_state.favorite_event_slugs = favorite_slugs;
        app.favorites_state.is_loading = false;
        app.favorites_state.selected_index = 0;
        app.favorites_state.scroll = 0;
    });
}

/// Spawn async task to search events and calculate yield for each
fn spawn_yield_search(app_state: Arc<TokioMutex<TrendingAppState>>, query: String) {
    use polymarket_api::GammaClient;

    tokio::spawn(async move {
        let min_prob = {
            let mut app = app_state.lock().await;
            app.yield_state.is_search_loading = true;
            app.yield_state.min_prob
        };

        log_info!("Yield search for: '{}'", query);

        let gamma_client = GammaClient::new();

        // Search for events
        let events = match gamma_client.search_events(&query, Some(50)).await {
            Ok(e) => e,
            Err(e) => {
                log_error!("Yield search failed: {}", e);
                let mut app = app_state.lock().await;
                app.yield_state.is_search_loading = false;
                return;
            },
        };

        log_info!("Yield search found {} events", events.len());

        // Convert events to YieldSearchResults with yield info
        let mut results: Vec<YieldSearchResult> = events
            .iter()
            .map(|event| {
                // Parse end date for filtering/sorting
                let end_date = event
                    .end_date
                    .as_ref()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                // Find best yield opportunity across all markets
                let mut best_yield: Option<YieldOpportunity> = None;

                for market in &event.markets {
                    if market.closed {
                        continue;
                    }

                    for (i, price_str) in market.outcome_prices.iter().enumerate() {
                        if let Ok(price) = price_str.parse::<f64>() {
                            // Only consider high-probability outcomes (>= min_prob)
                            if price >= min_prob {
                                let outcome = market
                                    .outcomes
                                    .get(i)
                                    .cloned()
                                    .unwrap_or_else(|| format!("Outcome {}", i));
                                let est_return = (1.0 - price) * 100.0;
                                let volume = market.volume_24hr.unwrap_or(0.0);

                                let market_name = market
                                    .group_item_title
                                    .as_ref()
                                    .filter(|s| !s.is_empty())
                                    .cloned()
                                    .unwrap_or_else(|| market.question.clone());

                                let opp = YieldOpportunity {
                                    market_name,
                                    market_status: market.status(),
                                    outcome,
                                    price,
                                    est_return,
                                    volume,
                                    event_slug: event.slug.clone(),
                                    event_title: event.title.clone(),
                                    end_date,
                                };

                                // Keep the best (highest return) opportunity
                                if best_yield
                                    .as_ref()
                                    .map(|b| opp.est_return > b.est_return)
                                    .unwrap_or(true)
                                {
                                    best_yield = Some(opp);
                                }
                            }
                        }
                    }
                }

                YieldSearchResult {
                    event_slug: event.slug.clone(),
                    best_yield,
                }
            })
            .collect();

        let query_clone = query.clone();
        let mut app = app_state.lock().await;
        
        // Cache all events from search results
        app.cache_events(&events);
        
        // Sort: events with yield first (by return), then events without yield (by volume from cache)
        results.sort_by(|a, b| {
            match (&a.best_yield, &b.best_yield) {
                (Some(ya), Some(yb)) => yb
                    .est_return
                    .partial_cmp(&ya.est_return)
                    .unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less, // a (with yield) comes first
                (None, Some(_)) => std::cmp::Ordering::Greater, // b (with yield) comes first
                (None, None) => {
                    // Look up volumes from cache
                    let vol_a = app.get_cached_event(&a.event_slug)
                        .map(|e| e.markets.iter().map(|m| m.volume_24hr.unwrap_or(0.0)).sum::<f64>())
                        .unwrap_or(0.0);
                    let vol_b = app.get_cached_event(&b.event_slug)
                        .map(|e| e.markets.iter().map(|m| m.volume_24hr.unwrap_or(0.0)).sum::<f64>())
                        .unwrap_or(0.0);
                    vol_b.partial_cmp(&vol_a).unwrap_or(std::cmp::Ordering::Equal)
                }
            }
        });
        
        app.yield_state.set_search_results(results, query_clone);

        log_info!(
            "Yield search complete: {} results",
            app.yield_state.search_results.len()
        );
    });
}

pub async fn run_trending_tui(
    mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
    app_state: Arc<TokioMutex<TrendingAppState>>,
) -> anyhow::Result<Option<String>> {
    use {
        crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind},
        polymarket_api::{GammaClient, RTDSClient},
    };

    let mut search_debounce: Option<tokio::time::Instant> = None;
    let mut yield_search_debounce: Option<tokio::time::Instant> = None;
    let mut last_selected_event_slug: Option<String> = None;
    let mut last_click: Option<(tokio::time::Instant, u16, u16)> = None; // (time, column, row)

    // Load saved auth config on startup
    if let Some(auth_config) = crate::auth::AuthConfig::load() {
        let address = auth_config.address.clone();
        let has_username = auth_config.username.is_some();
        {
            let mut app = app_state.lock().await;
            let short_addr = auth_config.short_address();
            app.auth_state.is_authenticated = true;
            app.auth_state.address = Some(auth_config.address);
            app.auth_state.username = auth_config.username;
            app.has_clob_auth = true;
            log_info!("Loaded saved auth config for {}", short_addr);
        }

        // If no username saved, fetch profile from API
        if !has_username {
            spawn_fetch_user_profile(Arc::clone(&app_state), address.clone());
        }

        // Fetch portfolio data (balance, positions) in background
        spawn_fetch_portfolio(Arc::clone(&app_state), address);

        // Load favorites in background at startup
        spawn_fetch_favorites(Arc::clone(&app_state));
    }

    // Fetch trade counts for the initially selected event (if authenticated)
    {
        let app = app_state.lock().await;
        if app.has_clob_auth
            && let Some(event) = app.selected_event()
        {
            let current_slug = event.slug.clone();
            let condition_ids: Vec<String> =
                event.markets.iter().filter_map(|m| m.id.clone()).collect();
            if !condition_ids.is_empty() {
                let app_state_clone = Arc::clone(&app_state);
                let slug_clone = current_slug.clone();
                let clob_client = ClobClient::from_env();
                last_selected_event_slug = Some(current_slug);

                tokio::spawn(async move {
                    if let Some(count) = fetch_event_trade_count(&clob_client, condition_ids).await
                    {
                        let mut app = app_state_clone.lock().await;
                        app.event_trade_counts.insert(slug_clone, count);
                        log_info!("Fetched initial trade count: {} trades", count);
                    }
                });
            }
        }
    }

    // Preload data for all filter tabs (Trending, Breaking, New)
    {
        let app = app_state.lock().await;
        let current_filter = app.event_filter;
        let limit = app.pagination.current_limit;

        // Preload the filter that isn't currently loaded
        let filters_to_preload: Vec<EventFilter> =
            [EventFilter::Trending, EventFilter::Breaking]
                .into_iter()
                .filter(|f| *f != current_filter)
                .collect();

        for filter in filters_to_preload {
            let app_state_clone = Arc::clone(&app_state);
            let gamma_client = GammaClient::new();

            tokio::spawn(async move {
                match fetch_events_for_filter(&gamma_client, filter, limit).await {
                    Ok(events) => {
                        let mut app = app_state_clone.lock().await;
                        // Only cache if not already cached (in case user switched tabs quickly)
                        app.events_cache.entry(filter).or_insert_with(|| {
                            log_info!(
                                "Preloaded {} events for {} filter",
                                events.len(),
                                filter.label()
                            );
                            events
                        });
                    },
                    Err(e) => {
                        log_error!("Failed to preload {} filter: {}", filter.label(), e);
                    },
                }
            });
        }
    }

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
                    log_info!("Searching for: '{}'", query);
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
                        log_info!("[TASK] Starting search for: '{}'", query_clone);

                        let result = gamma_client_for_task
                            .search_events(&query_clone, Some(50))
                            .await;

                        match result {
                            Ok(results) => {
                                log_info!("Search found {} results", results.len());
                                let mut app = app_state_clone.lock().await;
                                app.set_search_results(results, query_clone);
                            },
                            Err(_e) => {
                                log_error!("Search failed: {}", _e);
                                let mut app = app_state_clone.lock().await;
                                app.set_searching(false);
                                app.search.results.clear();
                            },
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

        // Handle yield search debouncing
        if let Some(debounce_time) = yield_search_debounce {
            let elapsed = debounce_time.elapsed();
            if elapsed >= tokio::time::Duration::from_millis(500) {
                let query = {
                    let app = app_state.lock().await;
                    app.yield_state.search_query.clone()
                };

                yield_search_debounce = None;

                if !query.is_empty() {
                    {
                        let mut app = app_state.lock().await;
                        app.yield_state.is_search_loading = true;
                    }
                    spawn_yield_search(Arc::clone(&app_state), query);
                } else {
                    let mut app = app_state.lock().await;
                    app.yield_state.search_results.clear();
                    app.yield_state.last_searched_query.clear();
                    app.yield_state.is_search_loading = false;
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
            let event = event::read()?;

            // Handle mouse events
            if let Event::Mouse(mouse) = &event {
                // Skip mouse handling when a popup/modal is active
                // The modal takes focus and background should not respond to mouse
                {
                    let app = app_state.lock().await;
                    if app.popup.is_some() {
                        continue;
                    }
                }

                // Focus panel on hover (mouse move)
                if let MouseEventKind::Moved = mouse.kind {
                    let mut app = app_state.lock().await;
                    let term_size = terminal.size()?;
                    let size = Rect::new(0, 0, term_size.width, term_size.height);
                    if let Some(panel) = get_panel_at_position(
                        mouse.column,
                        mouse.row,
                        size,
                        app.is_in_filter_mode(),
                        app.show_logs,
                        app.main_tab,
                    ) {
                        app.navigation.focused_panel = panel;
                    }
                }
                // Click to select event in events list, double-click to toggle watching
                if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                    let now = tokio::time::Instant::now();
                    let is_double_click = if let Some((last_time, last_col, last_row)) = last_click
                    {
                        now.duration_since(last_time) < tokio::time::Duration::from_millis(500)
                            && (mouse.column as i16 - last_col as i16).abs() <= 2
                            && mouse.row == last_row
                    } else {
                        false
                    };
                    last_click = Some((now, mouse.column, mouse.row));

                    let mut app = app_state.lock().await;
                    let term_size = terminal.size()?;
                    let size = Rect::new(0, 0, term_size.width, term_size.height);

                    // Check for login button click (top right)
                    if render::is_login_button_clicked(mouse.column, mouse.row, size, &app) {
                        if app.auth_state.is_authenticated {
                            app.show_popup(PopupType::UserProfile);
                        } else {
                            app.show_popup(PopupType::Login);
                        }
                        continue;
                    }

                    // Check for tab clicks (first line - unified tabs)
                    if let Some(clicked_tab) =
                        render::get_clicked_tab(mouse.column, mouse.row, size, &app)
                    {
                        match clicked_tab {
                            ClickedTab::Trending => {
                                if app.main_tab != MainTab::Trending
                                    || app.event_filter != EventFilter::Trending
                                {
                                    app.main_tab = MainTab::Trending;
                                    if let Some((filter, limit)) =
                                        switch_filter_tab(&mut app, EventFilter::Trending)
                                    {
                                        drop(app);
                                        spawn_filter_fetch(Arc::clone(&app_state), filter, limit);
                                    }
                                }
                            },
                            ClickedTab::Breaking => {
                                if app.main_tab != MainTab::Trending
                                    || app.event_filter != EventFilter::Breaking
                                {
                                    app.main_tab = MainTab::Trending;
                                    if let Some((filter, limit)) =
                                        switch_filter_tab(&mut app, EventFilter::Breaking)
                                    {
                                        drop(app);
                                        spawn_filter_fetch(Arc::clone(&app_state), filter, limit);
                                    }
                                }
                            },
                            ClickedTab::Favorites => {
                                if app.main_tab != MainTab::Favorites {
                                    app.main_tab = MainTab::Favorites;
                                    // If switching to Favorites tab and no data loaded, fetch it
                                    if app.favorites_state.events.is_empty()
                                        && !app.favorites_state.is_loading
                                        && app.auth_state.is_authenticated
                                    {
                                        drop(app);
                                        spawn_fetch_favorites(Arc::clone(&app_state));
                                    }
                                }
                            },
                            ClickedTab::Yield => {
                                if app.main_tab != MainTab::Yield {
                                    app.main_tab = MainTab::Yield;
                                    // If switching to Yield tab and no data loaded, fetch it
                                    if app.yield_state.opportunities.is_empty()
                                        && !app.yield_state.is_loading
                                    {
                                        drop(app);
                                        spawn_yield_fetch(Arc::clone(&app_state));
                                    }
                                }
                            },
                        }
                        continue;
                    }

                    let (_, events_list_area, ..) = calculate_panel_areas(
                        size,
                        app.is_in_filter_mode(),
                        app.show_logs,
                        app.main_tab,
                    );

                    if let Some(panel) = get_panel_at_position(
                        mouse.column,
                        mouse.row,
                        size,
                        app.is_in_filter_mode(),
                        app.show_logs,
                        app.main_tab,
                    ) {
                        // If clicking in events list, select the clicked item
                        if panel == FocusedPanel::EventsList {
                            if app.main_tab == MainTab::Yield {
                                // Yield tab: select yield opportunity or search result
                                // Account for border (1) + header row (1) = 2
                                // When searching/filtering, add 3 more for the input field
                                let extra_offset = if app.yield_state.is_searching
                                    || app.yield_state.is_filtering
                                {
                                    3
                                } else {
                                    0
                                };
                                let relative_y = mouse
                                    .row
                                    .saturating_sub(events_list_area.y + 2 + extra_offset)
                                    as usize;
                                let clicked_index = app.yield_state.scroll + relative_y;

                                // Determine the total items based on mode
                                let total_items = if !app.yield_state.search_results.is_empty() {
                                    app.yield_state.search_results.len()
                                } else {
                                    app.yield_state.filtered_opportunities().len()
                                };

                                if clicked_index < total_items {
                                    app.yield_state.selected_index = clicked_index;
                                }
                            } else {
                                // Trending tab: select event (List widget, no header row)
                                // Account for border (1) = 1
                                let relative_y = mouse.row.saturating_sub(events_list_area.y + 1);
                                let clicked_index = app.scroll.events_list + relative_y as usize;
                                let filtered_len = app.filtered_events().len();

                                if clicked_index < filtered_len {
                                    app.navigation.selected_index = clicked_index;
                                    // Reset markets scroll when changing events
                                    app.scroll.markets = 0;

                                    // Double-click toggles watching (same as Enter)
                                    if is_double_click
                                        && let Some(event_slug) = app.selected_event_slug()
                                    {
                                        if app.is_watching(&event_slug) {
                                            // Stop watching
                                            app.stop_watching(&event_slug);
                                        } else {
                                            // Start watching
                                            let event_slug_clone = event_slug.clone();

                                            app.trades
                                                .event_trades
                                                .entry(event_slug_clone.clone())
                                                .or_insert_with(EventTrades::new);

                                            let app_state_ws = Arc::clone(&app_state);
                                            let event_slug_for_closure = event_slug_clone.clone();

                                            let rtds_client = RTDSClient::new()
                                                .with_event_slug(event_slug_clone.clone());

                                            log_info!(
                                                "Starting RTDS WebSocket for event: {}",
                                                event_slug_clone
                                            );

                                            let ws_handle = tokio::spawn(async move {
                                                match rtds_client
                                                    .connect_and_listen(move |msg| {
                                                        let app_state = Arc::clone(&app_state_ws);
                                                        let event_slug =
                                                            event_slug_for_closure.clone();

                                                        tokio::spawn(async move {
                                                            let mut app = app_state.lock().await;
                                                            if let Some(event_trades) = app
                                                                .trades
                                                                .event_trades
                                                                .get_mut(&event_slug)
                                                            {
                                                                event_trades.add_trade(&msg);
                                                            }
                                                        });
                                                    })
                                                    .await
                                                {
                                                    Ok(()) => {},
                                                    Err(_e) => {
                                                        log_error!("RTDS WebSocket error: {}", _e);
                                                    },
                                                }
                                            });

                                            app.start_watching(event_slug_clone, ws_handle);
                                        }
                                    }
                                }
                            }
                        }
                        // Handle click on Markets panel to open trade popup
                        if panel == FocusedPanel::Markets && app.main_tab == MainTab::Trending {
                            // Extract trade popup data before mutably borrowing app
                            let trade_data: Option<(String, String, String, f64)> =
                                if let Some(event) = app.selected_event() {
                                    // Calculate which market row was clicked
                                    let (_, _, _, markets_area, ..) = calculate_panel_areas(
                                        size,
                                        app.is_in_filter_mode(),
                                        app.show_logs,
                                        app.main_tab,
                                    );
                                    // Account for border (1 line at top)
                                    let relative_y =
                                        mouse.row.saturating_sub(markets_area.y + 1) as usize;
                                    let clicked_idx = app.scroll.markets + relative_y;

                                    // Sort markets same way as render_markets (non-closed first)
                                    let mut sorted_markets: Vec<_> = event.markets.iter().collect();
                                    sorted_markets.sort_by_key(|m| m.closed);

                                    if clicked_idx < sorted_markets.len() {
                                        let market = sorted_markets[clicked_idx];
                                        // Only allow trading on non-closed markets
                                        if !market.closed {
                                            // Get the first outcome's token_id and price
                                            if let Some(ref token_ids) = market.clob_token_ids {
                                                if let Some(token_id) = token_ids.first() {
                                                    let outcome = market
                                                        .outcomes
                                                        .first()
                                                        .cloned()
                                                        .unwrap_or_else(|| "Yes".to_string());
                                                    let price = app
                                                        .market_prices
                                                        .get(token_id)
                                                        .copied()
                                                        .or_else(|| {
                                                            market
                                                                .outcome_prices
                                                                .first()
                                                                .and_then(|p| p.parse::<f64>().ok())
                                                        })
                                                        .unwrap_or(0.5);
                                                    Some((
                                                        token_id.clone(),
                                                        market.question.clone(),
                                                        outcome,
                                                        price,
                                                    ))
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                            // Now mutably borrow app to open the trade popup
                            if let Some((token_id, question, outcome, price)) = trade_data {
                                app.open_trade_popup(token_id, question.clone(), outcome, price);
                                log_info!("Opening trade popup for: {}", question);
                            }
                        }
                    }
                }
                // Handle scroll wheel
                if let MouseEventKind::ScrollUp = mouse.kind {
                    let mut app = app_state.lock().await;
                    let term_size = terminal.size()?;
                    let size = Rect::new(0, 0, term_size.width, term_size.height);
                    if let Some(panel) = get_panel_at_position(
                        mouse.column,
                        mouse.row,
                        size,
                        app.is_in_filter_mode(),
                        app.show_logs,
                        app.main_tab,
                    ) {
                        match panel {
                            FocusedPanel::EventsList => {
                                // In Yield tab, scroll yield list or search results
                                if app.main_tab == MainTab::Yield {
                                    app.yield_state.move_up();
                                } else {
                                    app.move_up();
                                }
                            },
                            FocusedPanel::EventDetails => {
                                if app.scroll.event_details > 0 {
                                    app.scroll.event_details -= 1;
                                }
                            },
                            FocusedPanel::Markets => {
                                if app.scroll.markets > 0 {
                                    app.scroll.markets -= 1;
                                }
                            },
                            FocusedPanel::Trades => {
                                if app.scroll.trades > 0 {
                                    app.scroll.trades -= 1;
                                }
                            },
                            FocusedPanel::Logs => {
                                if app.logs.scroll > 0 {
                                    app.logs.scroll -= 1;
                                }
                            },
                            _ => {},
                        }
                    }
                }
                if let MouseEventKind::ScrollDown = mouse.kind {
                    let mut app = app_state.lock().await;
                    let term_size = terminal.size()?;
                    let size = Rect::new(0, 0, term_size.width, term_size.height);
                    if let Some(panel) = get_panel_at_position(
                        mouse.column,
                        mouse.row,
                        size,
                        app.is_in_filter_mode(),
                        app.show_logs,
                        app.main_tab,
                    ) {
                        match panel {
                            FocusedPanel::EventsList => {
                                // In Yield tab, scroll yield list or search results
                                if app.main_tab == MainTab::Yield {
                                    // Use approximate visible height for yield list
                                    let visible_height = 20;
                                    app.yield_state.move_down(visible_height);
                                } else {
                                    app.move_down();
                                    // Check if we need to fetch more events (infinite scroll)
                                    if app.should_fetch_more() {
                                        let app_state_clone = Arc::clone(&app_state);
                                        let gamma_client_clone = GammaClient::new();
                                        let current_filter = app.event_filter;
                                        let current_limit = app.pagination.current_limit;

                                        // Set fetching flag to prevent duplicate requests
                                        app.pagination.is_fetching_more = true;

                                        // Fetch 50 more events
                                        let new_limit = current_limit + 50;
                                        log_info!("Fetching more events (limit: {})", new_limit);

                                        tokio::spawn(async move {
                                            match fetch_events_for_filter(
                                                &gamma_client_clone,
                                                current_filter,
                                                new_limit,
                                            )
                                            .await
                                            {
                                                Ok(mut new_events) => {
                                                    // Remove duplicates by comparing slugs
                                                    let existing_slugs: std::collections::HashSet<
                                                        _,
                                                    > = {
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
                                                        log_info!(
                                                            "Fetched {} new events",
                                                            new_events.len()
                                                        );
                                                        let mut app = app_state_clone.lock().await;
                                                        app.events.append(&mut new_events);
                                                        app.pagination.current_limit = new_limit;
                                                    } else {
                                                        log_info!(
                                                            "No new events to add (already have all events)"
                                                        );
                                                    }

                                                    let mut app = app_state_clone.lock().await;
                                                    app.pagination.is_fetching_more = false;
                                                },
                                                Err(_e) => {
                                                    log_error!(
                                                        "Failed to fetch more events: {}",
                                                        _e
                                                    );
                                                    let mut app = app_state_clone.lock().await;
                                                    app.pagination.is_fetching_more = false;
                                                },
                                            }
                                        });
                                    }
                                }
                            },
                            FocusedPanel::EventDetails => {
                                app.scroll.event_details += 1;
                            },
                            FocusedPanel::Markets => {
                                if let Some(event) = app.selected_event() {
                                    let visible_height: usize = 5;
                                    if app.scroll.markets
                                        < event.markets.len().saturating_sub(visible_height)
                                    {
                                        app.scroll.markets += 1;
                                    }
                                }
                            },
                            FocusedPanel::Trades => {
                                let trades_len = if let Some(event) = app.selected_event() {
                                    app.get_trades(&event.slug).len()
                                } else {
                                    0
                                };
                                let visible_height: usize = 10;
                                if app.scroll.trades < trades_len.saturating_sub(visible_height) {
                                    app.scroll.trades += 1;
                                }
                            },
                            FocusedPanel::Logs => {
                                let visible_height: usize = 10;
                                let max_scroll = app
                                    .logs
                                    .messages
                                    .len()
                                    .saturating_sub(visible_height.max(1));
                                if app.logs.scroll < max_scroll {
                                    app.logs.scroll += 1;
                                }
                            },
                            _ => {},
                        }
                    }
                }
            }

            // Handle key events
            if let Event::Key(key) = event {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let mut app = app_state.lock().await;

                // Handle Login popup input
                if matches!(app.popup, Some(PopupType::Login)) {
                    match key.code {
                        KeyCode::Esc => {
                            app.login_form.clear();
                            app.close_popup();
                        },
                        KeyCode::Tab | KeyCode::Down => {
                            app.login_form.active_field = app.login_form.active_field.next();
                        },
                        KeyCode::BackTab | KeyCode::Up => {
                            app.login_form.active_field = app.login_form.active_field.prev();
                        },
                        KeyCode::Backspace => {
                            app.login_form.delete_char();
                        },
                        KeyCode::Enter => {
                            // Validate and save credentials
                            // Convert empty strings to None for optional cookie fields
                            let session_cookie = if app.login_form.session_cookie.is_empty() {
                                None
                            } else {
                                Some(app.login_form.session_cookie.clone())
                            };
                            let session_nonce = if app.login_form.session_nonce.is_empty() {
                                None
                            } else {
                                Some(app.login_form.session_nonce.clone())
                            };
                            let session_auth_type = if app.login_form.session_auth_type.is_empty() {
                                None
                            } else {
                                Some(app.login_form.session_auth_type.clone())
                            };

                            let config = crate::auth::AuthConfig {
                                api_key: app.login_form.api_key.clone(),
                                secret: app.login_form.secret.clone(),
                                passphrase: app.login_form.passphrase.clone(),
                                address: app.login_form.address.clone(),
                                username: None,
                                session_cookie,
                                session_nonce,
                                session_auth_type,
                            };

                            match config.validate() {
                                Ok(()) => {
                                    // Save to config file
                                    match config.save() {
                                        Ok(()) => {
                                            // Update auth state
                                            let address_for_profile = config.address.clone();
                                            app.auth_state.is_authenticated = true;
                                            app.auth_state.address = Some(config.address.clone());
                                            app.auth_state.username = config.username.clone();
                                            app.has_clob_auth = true;
                                            app.login_form.clear();
                                            app.close_popup();
                                            log_info!("Logged in successfully");

                                            // Fetch user profile to get username
                                            drop(app); // Release lock before spawning
                                            spawn_fetch_user_profile(
                                                Arc::clone(&app_state),
                                                address_for_profile,
                                            );
                                        },
                                        Err(e) => {
                                            app.login_form.error_message = Some(e);
                                        },
                                    }
                                },
                                Err(e) => {
                                    app.login_form.error_message = Some(e);
                                },
                            }
                        },
                        KeyCode::Char(c) => {
                            app.login_form.add_char(c);
                        },
                        _ => {},
                    }
                    continue;
                }

                // Handle UserProfile popup
                if matches!(app.popup, Some(PopupType::UserProfile)) {
                    match key.code {
                        KeyCode::Esc => {
                            app.close_popup();
                        },
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            // Logout
                            let _ = crate::auth::AuthConfig::delete();
                            app.auth_state.is_authenticated = false;
                            app.auth_state.address = None;
                            app.auth_state.username = None;
                            app.auth_state.balance = None;
                            app.has_clob_auth = false;
                            app.close_popup();
                            log_info!("Logged out");
                        },
                        _ => {},
                    }
                    continue;
                }

                // Handle Trade popup input
                if matches!(app.popup, Some(PopupType::Trade)) {
                    // Check auth state before borrowing trade_form mutably
                    let is_authenticated = app.auth_state.is_authenticated;
                    let mut should_close = false;

                    if let Some(ref mut form) = app.trade_form {
                        match key.code {
                            KeyCode::Esc => {
                                should_close = true;
                            },
                            KeyCode::Tab => {
                                form.active_field = form.active_field.next();
                            },
                            KeyCode::BackTab => {
                                form.active_field = form.active_field.prev();
                            },
                            KeyCode::Char(' ') => {
                                // Space toggles buy/sell side
                                form.toggle_side();
                            },
                            KeyCode::Backspace => {
                                form.delete_char();
                            },
                            KeyCode::Enter => {
                                // Validate and submit trade
                                if !is_authenticated {
                                    form.error_message =
                                        Some("Login required to trade".to_string());
                                } else if form.amount.is_empty() || form.amount_f64() <= 0.0 {
                                    form.error_message =
                                        Some("Please enter a valid amount".to_string());
                                } else {
                                    // TODO: Actually submit the trade via CLOB API
                                    log_info!(
                                        "Trade submitted: {} ${} of {} at {:.0}¢",
                                        form.side.label(),
                                        form.amount,
                                        form.outcome,
                                        form.price * 100.0
                                    );
                                    form.error_message =
                                        Some("Trade submission not yet implemented".to_string());
                                    // For now, just close
                                    // should_close = true;
                                }
                            },
                            KeyCode::Char(c) => {
                                form.add_char(c);
                            },
                            _ => {},
                        }
                    } else {
                        // No form state, close popup
                        should_close = true;
                    }

                    if should_close {
                        app.close_popup();
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => {
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            // In yield search mode, 'q' adds to search (common letter)
                            app.yield_state.add_search_char('q');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            // In yield filter mode, 'q' adds to filter (common letter)
                            app.yield_state.add_filter_char('q');
                        } else if app.is_in_filter_mode() {
                            app.exit_search_mode();
                        } else {
                            app.should_quit = true;
                            break;
                        }
                    },
                    KeyCode::Esc => {
                        // Close popup first, then yield search/filter mode, then search/filter mode, then quit
                        if app.has_popup() {
                            app.close_popup();
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.exit_search_mode();
                            log_info!("Exited yield search mode");
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.exit_filter_mode();
                            log_info!("Exited yield filter mode");
                        } else if app.is_in_filter_mode() {
                            app.exit_search_mode();
                        } else {
                            app.should_quit = true;
                            break;
                        }
                    },
                    KeyCode::Char('?') => {
                        // Show help popup (unless in search/filter mode)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('?');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('?');
                        } else if !app.is_in_filter_mode() {
                            app.show_popup(state::PopupType::Help);
                        }
                    },
                    KeyCode::Char('1') => {
                        // Switch to Trending tab (unless in search/filter mode)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('1');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('1');
                        } else if !app.is_in_filter_mode() {
                            if app.main_tab != MainTab::Trending
                                || app.event_filter != EventFilter::Trending
                            {
                                app.main_tab = MainTab::Trending;
                                if let Some((filter, limit)) =
                                    switch_filter_tab(&mut app, EventFilter::Trending)
                                {
                                    drop(app);
                                    spawn_filter_fetch(Arc::clone(&app_state), filter, limit);
                                }
                                log_info!("Switched to Trending tab");
                            }
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('1');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        }
                    },
                    KeyCode::Char('2') => {
                        // Switch to Favorites tab (unless in search/filter mode)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('2');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('2');
                        } else if !app.is_in_filter_mode() && app.main_tab != MainTab::Favorites {
                            app.main_tab = MainTab::Favorites;
                            // Fetch favorites if not already loaded
                            if app.favorites_state.events.is_empty()
                                && !app.favorites_state.is_loading
                                && app.auth_state.is_authenticated
                            {
                                drop(app);
                                spawn_fetch_favorites(Arc::clone(&app_state));
                            }
                            log_info!("Switched to Favorites tab");
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('2');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        }
                    },
                    KeyCode::Char('3') => {
                        // Switch to Breaking tab (unless in search/filter mode)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('3');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('3');
                        } else if !app.is_in_filter_mode() {
                            if app.main_tab != MainTab::Trending
                                || app.event_filter != EventFilter::Breaking
                            {
                                app.main_tab = MainTab::Trending;
                                if let Some((filter, limit)) =
                                    switch_filter_tab(&mut app, EventFilter::Breaking)
                                {
                                    drop(app);
                                    spawn_filter_fetch(Arc::clone(&app_state), filter, limit);
                                }
                                log_info!("Switched to Breaking tab");
                            }
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('3');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        }
                    },
                    KeyCode::Char('4') => {
                        // Switch to Yield tab (unless in search/filter mode)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('4');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('4');
                        } else if !app.is_in_filter_mode() && app.main_tab != MainTab::Yield {
                            app.main_tab = MainTab::Yield;
                            // Fetch yield data if not already loaded
                            if app.yield_state.opportunities.is_empty()
                                && !app.yield_state.is_loading
                            {
                                drop(app);
                                spawn_yield_fetch(Arc::clone(&app_state));
                            }
                            log_info!("Switched to Yield tab");
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('4');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        }
                    },
                    KeyCode::Char('5') => {
                        // '5' is now just a regular character (no tab assigned)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('5');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('5');
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('5');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        }
                    },
                    KeyCode::Char('l') => {
                        // Toggle logs panel visibility (disabled in filter/search mode)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('l');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('l');
                        } else if !app.is_in_filter_mode() {
                            app.show_logs = !app.show_logs;
                            // If hiding logs and logs panel was focused, switch to another panel
                            if !app.show_logs && app.navigation.focused_panel == FocusedPanel::Logs
                            {
                                app.navigation.focused_panel = FocusedPanel::EventsList;
                            }
                        } else {
                            // In filter mode, add 'l' to search query
                            app.add_search_char('l');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        }
                    },
                    KeyCode::Char('p') => {
                        // Show profile popup (if authenticated and not in search/filter mode)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('p');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('p');
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('p');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        } else if app.auth_state.is_authenticated {
                            app.show_popup(state::PopupType::UserProfile);
                        }
                    },
                    KeyCode::Char('b') => {
                        // Toggle bookmark/favorite for current event
                        // Skip if in search/filter mode or if popup is open
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('b');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('b');
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('b');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        } else if !app.has_popup() && app.auth_state.is_authenticated {
                            // Get the event to toggle based on current tab
                            match app.main_tab {
                                MainTab::Trending | MainTab::Favorites => {
                                    if let Some(e) = app.selected_event() {
                                        spawn_toggle_favorite(
                                            Arc::clone(&app_state),
                                            e.id.clone(),
                                            e.slug.clone(),
                                            Some(e.clone()),
                                        );
                                    }
                                },
                                MainTab::Yield => {
                                    // For yield tab, get event_slug from selected opportunity
                                    // We need to fetch the event to get the ID
                                    if let Some(opp) = app.yield_state.selected_opportunity() {
                                        let event_slug = opp.event_slug.clone();
                                        // Try to find it in the events cache or favorites
                                        let cached_event = app
                                            .events
                                            .iter()
                                            .find(|e| e.slug == event_slug)
                                            .cloned()
                                            .or_else(|| {
                                                app.favorites_state
                                                    .events
                                                    .iter()
                                                    .find(|e| e.slug == event_slug)
                                                    .cloned()
                                            });

                                        if let Some(event) = cached_event {
                                            spawn_toggle_favorite(
                                                Arc::clone(&app_state),
                                                event.id.clone(),
                                                event.slug.clone(),
                                                Some(event),
                                            );
                                        } else {
                                            // Event not in cache, fetch it first then toggle
                                            spawn_fetch_and_toggle_favorite(
                                                Arc::clone(&app_state),
                                                event_slug,
                                            );
                                        }
                                    }
                                },
                            };
                        }
                    },
                    KeyCode::Char('/') => {
                        // API search mode - works from any panel (except when popup is open)
                        if app.main_tab == MainTab::Yield {
                            if app.yield_state.is_filtering {
                                // If in filter mode, add '/' to filter
                                app.yield_state.add_filter_char('/');
                            } else if !app.yield_state.is_searching {
                                // Enter yield search mode
                                app.yield_state.enter_search_mode();
                                log_info!("Entered yield search mode");
                            }
                        } else if app.is_in_filter_mode() {
                            // Already in search/filter mode, add '/' to query
                            app.add_search_char('/');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        } else if !app.has_popup() {
                            // API search in Trending/Favorites tab from any panel
                            app.enter_search_mode();
                        }
                    },
                    KeyCode::Char('f') => {
                        // Local filter - works from any panel (except when popup is open)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            // In yield search mode, add 'f' to search query
                            app.yield_state.add_search_char('f');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield {
                            if !app.yield_state.is_filtering {
                                app.yield_state.enter_filter_mode();
                                log_info!("Entered yield filter mode");
                            } else {
                                // Already filtering, add 'f' to filter query
                                app.yield_state.add_filter_char('f');
                            }
                        } else if app.is_in_filter_mode() {
                            // Already in search/filter mode, add 'f' to query
                            app.add_search_char('f');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        } else if !app.has_popup() {
                            // Local filter in Trending/Favorites tab from any panel
                            app.enter_local_filter_mode();
                        }
                    },
                    KeyCode::Char('o') => {
                        // Open event URL in browser
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('o');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('o');
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('o');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        } else if !app.has_popup() {
                            // Open event URL in browser (works from any panel, any tab)
                            let event_slug: Option<String> = match app.main_tab {
                                MainTab::Yield => app
                                    .yield_state
                                    .selected_opportunity()
                                    .map(|o| o.event_slug.clone()),
                                MainTab::Trending | MainTab::Favorites => {
                                    app.selected_event().map(|e| e.slug.clone())
                                },
                            };

                            if let Some(slug) = event_slug {
                                let url = format!("https://polymarket.com/event/{}", slug);
                                #[cfg(target_os = "macos")]
                                let _ = std::process::Command::new("open").arg(&url).spawn();
                                #[cfg(target_os = "linux")]
                                let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                                #[cfg(target_os = "windows")]
                                let _ = std::process::Command::new("cmd")
                                    .args(["/C", "start", &url])
                                    .spawn();
                            }
                        }
                    },
                    KeyCode::Char('e') => {
                        // Open config file in editor (only in Favorites tab when session cookie is missing)
                        if app.main_tab == MainTab::Favorites
                            && app.favorites_state.error_message.is_some()
                        {
                            let config_path = crate::auth::AuthConfig::config_path();
                            let config_path_str = config_path.display().to_string();

                            // Use system open command (opens in GUI editor, not terminal)
                            // We can't use terminal editors like vim/nvim while the TUI is running
                            #[cfg(target_os = "macos")]
                            let result = std::process::Command::new("open")
                                .arg("-t") // Open in default text editor (usually TextEdit)
                                .arg(&config_path_str)
                                .spawn();

                            #[cfg(target_os = "linux")]
                            let result = std::process::Command::new("xdg-open")
                                .arg(&config_path_str)
                                .spawn();

                            #[cfg(target_os = "windows")]
                            let result = std::process::Command::new("cmd")
                                .args(["/C", "start", "notepad", &config_path_str])
                                .spawn();

                            match result {
                                Ok(_) => log_info!("Opened config file: {}", config_path_str),
                                Err(e) => log_error!("Failed to open config file: {}", e),
                            }
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('e');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('e');
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('e');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        }
                    },
                    KeyCode::Char('s') => {
                        // Cycle sort order in Yield tab (or add to search/filter if in input mode)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('s');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('s');
                        } else if !app.is_in_filter_mode() && app.main_tab == MainTab::Yield {
                            app.yield_state.sort_by = app.yield_state.sort_by.next();
                            app.yield_state.sort_opportunities();
                            app.yield_state.selected_index = 0;
                            app.yield_state.scroll = 0;
                            log_info!("Sort changed to: {}", app.yield_state.sort_by.label());
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('s');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        }
                    },
                    KeyCode::Char('r') => {
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            // In yield search mode, add 'r' to search query
                            app.yield_state.add_search_char('r');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            // In yield filter mode, add 'r' to filter query
                            app.yield_state.add_filter_char('r');
                        } else if app.is_in_filter_mode() {
                            // In filter mode, add 'r' to search query
                            app.add_search_char('r');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        } else if app.main_tab == MainTab::Yield {
                            // Refresh yield opportunities
                            if !app.yield_state.is_loading {
                                log_info!("Refreshing yield opportunities...");
                                spawn_yield_fetch(Arc::clone(&app_state));
                                // Also refresh favorites in background
                                if app.auth_state.is_authenticated {
                                    spawn_fetch_favorites(Arc::clone(&app_state));
                                }
                            }
                        } else if app.main_tab == MainTab::Favorites {
                            // Refresh favorites list
                            if !app.favorites_state.is_loading && app.auth_state.is_authenticated {
                                log_info!("Refreshing favorites...");
                                spawn_fetch_favorites(Arc::clone(&app_state));
                            }
                        } else if app.navigation.focused_panel == FocusedPanel::EventsList {
                            // Refresh events list and update cache
                            let current_filter = app.event_filter;
                            let limit = app.pagination.current_limit;
                            let app_state_clone = Arc::clone(&app_state);
                            let gamma_client = GammaClient::new();
                            let is_authenticated = app.auth_state.is_authenticated;

                            log_info!("Refreshing events list...");

                            tokio::spawn(async move {
                                match fetch_events_for_filter(&gamma_client, current_filter, limit)
                                    .await
                                {
                                    Ok(new_events) => {
                                        let mut app = app_state_clone.lock().await;
                                        // Update cache for current filter
                                        app.events_cache.insert(current_filter, new_events.clone());
                                        app.events = new_events;
                                        log_info!("Events refreshed ({} events)", app.events.len());
                                    },
                                    Err(_e) => {
                                        log_info!("Failed to refresh events: {}", _e);
                                    },
                                }
                            });

                            // Also refresh favorites in background to sync bookmark icons
                            if is_authenticated {
                                spawn_fetch_favorites(Arc::clone(&app_state));
                            }
                        } else if app.navigation.focused_panel == FocusedPanel::Markets
                            && let Some(event) = app.selected_event()
                        {
                            // Refresh market prices
                            // Only fetch prices for active (non-closed) markets
                            let active_markets: Vec<_> = event
                                .markets
                                .iter()
                                .filter(|m| !m.closed)
                                .filter_map(|m| m.clob_token_ids.clone())
                                .collect();

                            let _active_count = active_markets.len();
                            let _closed_count = event.markets.iter().filter(|m| m.closed).count();
                            log_info!(
                                "Refreshing market prices for event: {} ({} active, {} resolved)",
                                event.slug,
                                _active_count,
                                _closed_count
                            );

                            if active_markets.is_empty() {
                                log_info!("No active markets to refresh");
                            } else {
                                let app_state_clone = Arc::clone(&app_state);
                                let clob_client = ClobClient::new();

                                tokio::spawn(async move {
                                    let prices =
                                        fetch_market_prices_batch(&clob_client, active_markets)
                                            .await;
                                    let mut app = app_state_clone.lock().await;
                                    app.market_prices.extend(prices);
                                    log_info!("Market prices refreshed via batch API");
                                });
                            }
                        }
                    },
                    KeyCode::Tab => {
                        if !app.is_in_filter_mode() {
                            // Cycle through panels, skipping Logs if hidden
                            app.navigation.focused_panel = match app.navigation.focused_panel {
                                FocusedPanel::Header => FocusedPanel::EventsList,
                                FocusedPanel::EventsList => FocusedPanel::EventDetails,
                                FocusedPanel::EventDetails => FocusedPanel::Markets,
                                FocusedPanel::Markets => FocusedPanel::Trades,
                                FocusedPanel::Trades => {
                                    if app.show_logs {
                                        FocusedPanel::Logs
                                    } else {
                                        FocusedPanel::Header
                                    }
                                },
                                FocusedPanel::Logs => FocusedPanel::Header,
                            };
                        }
                    },
                    KeyCode::Left => {
                        if !app.is_in_filter_mode()
                            && app.navigation.focused_panel == FocusedPanel::Header
                        {
                            // Cycle through all tabs: Yield -> Breaking -> Favorites -> Events -> Yield
                            match app.main_tab {
                                MainTab::Trending => {
                                    match app.event_filter {
                                        EventFilter::Trending => {
                                            // Wrap to Yield tab
                                            app.main_tab = MainTab::Yield;
                                            if app.yield_state.opportunities.is_empty()
                                                && !app.yield_state.is_loading
                                            {
                                                drop(app);
                                                spawn_yield_fetch(Arc::clone(&app_state));
                                            }
                                        },
                                        EventFilter::Breaking => {
                                            // Go to Favorites tab
                                            app.main_tab = MainTab::Favorites;
                                            if app.favorites_state.events.is_empty()
                                                && !app.favorites_state.is_loading
                                                && app.auth_state.is_authenticated
                                            {
                                                drop(app);
                                                spawn_fetch_favorites(Arc::clone(&app_state));
                                            }
                                        },
                                    }
                                },
                                MainTab::Favorites => {
                                    // Go to Events tab
                                    app.main_tab = MainTab::Trending;
                                    if let Some((filter, limit)) =
                                        switch_filter_tab(&mut app, EventFilter::Trending)
                                    {
                                        drop(app);
                                        spawn_filter_fetch(Arc::clone(&app_state), filter, limit);
                                    }
                                },
                                MainTab::Yield => {
                                    // Go to Breaking tab
                                    app.main_tab = MainTab::Trending;
                                    if let Some((filter, limit)) =
                                        switch_filter_tab(&mut app, EventFilter::Breaking)
                                    {
                                        drop(app);
                                        spawn_filter_fetch(Arc::clone(&app_state), filter, limit);
                                    }
                                },
                            }
                        }
                    },
                    KeyCode::Right => {
                        if !app.is_in_filter_mode()
                            && app.navigation.focused_panel == FocusedPanel::Header
                        {
                            // Cycle through all tabs: Events -> Favorites -> Breaking -> Yield -> Events
                            match app.main_tab {
                                MainTab::Trending => {
                                    match app.event_filter {
                                        EventFilter::Trending => {
                                            // Go to Favorites tab
                                            app.main_tab = MainTab::Favorites;
                                            if app.favorites_state.events.is_empty()
                                                && !app.favorites_state.is_loading
                                                && app.auth_state.is_authenticated
                                            {
                                                drop(app);
                                                spawn_fetch_favorites(Arc::clone(&app_state));
                                            }
                                        },
                                        EventFilter::Breaking => {
                                            // Go to Yield tab
                                            app.main_tab = MainTab::Yield;
                                            if app.yield_state.opportunities.is_empty()
                                                && !app.yield_state.is_loading
                                            {
                                                drop(app);
                                                spawn_yield_fetch(Arc::clone(&app_state));
                                            }
                                        },
                                    }
                                },
                                MainTab::Favorites => {
                                    // Go to Breaking tab
                                    app.main_tab = MainTab::Trending;
                                    if let Some((filter, limit)) =
                                        switch_filter_tab(&mut app, EventFilter::Breaking)
                                    {
                                        drop(app);
                                        spawn_filter_fetch(Arc::clone(&app_state), filter, limit);
                                    }
                                },
                                MainTab::Yield => {
                                    // Wrap to Events tab
                                    app.main_tab = MainTab::Trending;
                                    if let Some((filter, limit)) =
                                        switch_filter_tab(&mut app, EventFilter::Trending)
                                    {
                                        drop(app);
                                        spawn_filter_fetch(Arc::clone(&app_state), filter, limit);
                                    }
                                },
                            }
                        }
                    },
                    KeyCode::Up => {
                        if !app.is_in_filter_mode() {
                            // Handle favorites tab navigation
                            if app.main_tab == MainTab::Favorites {
                                app.favorites_state.move_up();
                                continue;
                            }
                            // Handle yield tab navigation
                            if app.main_tab == MainTab::Yield {
                                app.yield_state.move_up();
                                continue;
                            }
                            match app.navigation.focused_panel {
                                FocusedPanel::Header => {
                                    // Header doesn't scroll, but we can allow it for consistency
                                },
                                FocusedPanel::EventsList => {
                                    app.move_up();
                                    // Fetch market prices and trade counts when event selection changes
                                    if let Some(event) = app.selected_event() {
                                        let current_slug = event.slug.clone();
                                        if last_selected_event_slug.as_ref() != Some(&current_slug)
                                        {
                                            last_selected_event_slug = Some(current_slug.clone());
                                            // Only fetch prices for active (non-closed) markets
                                            let active_markets: Vec<_> = event
                                                .markets
                                                .iter()
                                                .filter(|m| !m.closed)
                                                .filter_map(|m| m.clob_token_ids.clone())
                                                .collect();

                                            if !active_markets.is_empty() {
                                                let app_state_clone = Arc::clone(&app_state);
                                                let clob_client = ClobClient::from_env();

                                                tokio::spawn(async move {
                                                    let prices = fetch_market_prices_batch(
                                                        &clob_client,
                                                        active_markets,
                                                    )
                                                    .await;
                                                    let mut app = app_state_clone.lock().await;
                                                    app.market_prices.extend(prices);
                                                });
                                            }

                                            // Fetch trade counts if authenticated and not already fetched
                                            let has_auth = app.has_clob_auth;
                                            let already_fetched =
                                                app.event_trade_counts.contains_key(&current_slug);
                                            if has_auth && !already_fetched {
                                                // Market ID is the condition_id used by CLOB API
                                                let condition_ids: Vec<String> = event
                                                    .markets
                                                    .iter()
                                                    .filter_map(|m| m.id.clone())
                                                    .collect();
                                                if !condition_ids.is_empty() {
                                                    let app_state_clone = Arc::clone(&app_state);
                                                    let slug_clone = current_slug.clone();
                                                    let clob_client = ClobClient::from_env();

                                                    tokio::spawn(async move {
                                                        if let Some(count) =
                                                            fetch_event_trade_count(
                                                                &clob_client,
                                                                condition_ids,
                                                            )
                                                            .await
                                                        {
                                                            let mut app =
                                                                app_state_clone.lock().await;
                                                            app.event_trade_counts
                                                                .insert(slug_clone, count);
                                                            log_info!(
                                                                "Fetched trade count: {} trades",
                                                                count
                                                            );
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                    }
                                },
                                FocusedPanel::EventDetails => {
                                    if app.scroll.event_details > 0 {
                                        app.scroll.event_details -= 1;
                                    }
                                },
                                FocusedPanel::Markets => {
                                    if app.scroll.markets > 0 {
                                        app.scroll.markets -= 1;
                                    }
                                },
                                FocusedPanel::Trades => {
                                    if app.scroll.trades > 0 {
                                        app.scroll.trades -= 1;
                                    }
                                },
                                FocusedPanel::Logs => {
                                    if app.logs.scroll > 0 {
                                        app.logs.scroll -= 1;
                                    }
                                },
                            }
                        }
                    },
                    KeyCode::Down => {
                        if !app.is_in_filter_mode() {
                            // Handle favorites tab navigation
                            if app.main_tab == MainTab::Favorites {
                                let visible_height = 20; // Approximate visible rows
                                app.favorites_state.move_down(visible_height);
                                continue;
                            }
                            // Handle yield tab navigation
                            if app.main_tab == MainTab::Yield {
                                // Calculate visible height (approximate)
                                let visible_height = 20; // Approximate visible rows
                                app.yield_state.move_down(visible_height);
                                continue;
                            }
                            match app.navigation.focused_panel {
                                FocusedPanel::Header => {
                                    // Header doesn't scroll, but we can allow it for consistency
                                },
                                FocusedPanel::EventsList => {
                                    app.move_down();
                                    // Fetch market prices and trade counts when event selection changes
                                    if let Some(event) = app.selected_event() {
                                        let current_slug = event.slug.clone();
                                        if last_selected_event_slug.as_ref() != Some(&current_slug)
                                        {
                                            last_selected_event_slug = Some(current_slug.clone());
                                            // Only fetch prices for active (non-closed) markets
                                            let active_markets: Vec<_> = event
                                                .markets
                                                .iter()
                                                .filter(|m| !m.closed)
                                                .filter_map(|m| m.clob_token_ids.clone())
                                                .collect();

                                            if !active_markets.is_empty() {
                                                let app_state_clone = Arc::clone(&app_state);
                                                let clob_client = ClobClient::from_env();

                                                tokio::spawn(async move {
                                                    let prices = fetch_market_prices_batch(
                                                        &clob_client,
                                                        active_markets,
                                                    )
                                                    .await;
                                                    let mut app = app_state_clone.lock().await;
                                                    app.market_prices.extend(prices);
                                                });
                                            }

                                            // Fetch trade counts if authenticated and not already fetched
                                            let has_auth = app.has_clob_auth;
                                            let already_fetched =
                                                app.event_trade_counts.contains_key(&current_slug);
                                            if has_auth && !already_fetched {
                                                // Market ID is the condition_id used by CLOB API
                                                let condition_ids: Vec<String> = event
                                                    .markets
                                                    .iter()
                                                    .filter_map(|m| m.id.clone())
                                                    .collect();
                                                if !condition_ids.is_empty() {
                                                    let app_state_clone = Arc::clone(&app_state);
                                                    let slug_clone = current_slug.clone();
                                                    let clob_client = ClobClient::from_env();

                                                    tokio::spawn(async move {
                                                        if let Some(count) =
                                                            fetch_event_trade_count(
                                                                &clob_client,
                                                                condition_ids,
                                                            )
                                                            .await
                                                        {
                                                            let mut app =
                                                                app_state_clone.lock().await;
                                                            app.event_trade_counts
                                                                .insert(slug_clone, count);
                                                            log_info!(
                                                                "Fetched trade count: {} trades",
                                                                count
                                                            );
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    // Check if we need to fetch more events (infinite scroll)
                                    if app.should_fetch_more() {
                                        let app_state_clone = Arc::clone(&app_state);
                                        let gamma_client_clone = GammaClient::new();
                                        let current_filter = app.event_filter;
                                        let current_limit = app.pagination.current_limit;

                                        // Set fetching flag to prevent duplicate requests
                                        app.pagination.is_fetching_more = true;

                                        // Fetch 50 more events
                                        let new_limit = current_limit + 50;
                                        log_info!("Fetching more events (limit: {})", new_limit);

                                        tokio::spawn(async move {
                                            match fetch_events_for_filter(
                                                &gamma_client_clone,
                                                current_filter,
                                                new_limit,
                                            )
                                            .await
                                            {
                                                Ok(mut new_events) => {
                                                    // Remove duplicates by comparing slugs
                                                    let existing_slugs: std::collections::HashSet<
                                                        _,
                                                    > = {
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
                                                        log_info!(
                                                            "Fetched {} new events",
                                                            new_events.len()
                                                        );
                                                        let mut app = app_state_clone.lock().await;
                                                        app.events.append(&mut new_events);
                                                        app.pagination.current_limit = new_limit;
                                                    } else {
                                                        log_info!(
                                                            "No new events to add (already have all events)"
                                                        );
                                                    }

                                                    let mut app = app_state_clone.lock().await;
                                                    app.pagination.is_fetching_more = false;
                                                },
                                                Err(_e) => {
                                                    log_error!(
                                                        "Failed to fetch more events: {}",
                                                        _e
                                                    );
                                                    let mut app = app_state_clone.lock().await;
                                                    app.pagination.is_fetching_more = false;
                                                },
                                            }
                                        });
                                    }
                                },
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
                                                let wrapped_lines =
                                                    tags_text.len().div_ceil(tags_content_width);
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
                                },
                                FocusedPanel::Markets => {
                                    if let Some(event) = app.selected_event() {
                                        let visible_height: usize = 5; // Markets panel height
                                        if app.scroll.markets
                                            < event.markets.len().saturating_sub(visible_height)
                                        {
                                            app.scroll.markets += 1;
                                        }
                                    }
                                },
                                FocusedPanel::Trades => {
                                    let trades_len = if let Some(event) = app.selected_event() {
                                        app.get_trades(&event.slug).len()
                                    } else {
                                        0
                                    };
                                    let visible_height: usize = 10; // Approximate
                                    if app.scroll.trades < trades_len.saturating_sub(visible_height)
                                    {
                                        app.scroll.trades += 1;
                                    }
                                },
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
                                },
                            }
                        }
                    },
                    KeyCode::Backspace => {
                        // Handle yield search mode
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.delete_search_char();
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        // Handle yield filter mode
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.delete_filter_char();
                        } else if app.is_in_filter_mode() {
                            app.delete_search_char();
                            // Trigger API search after backspace only if in API search mode (with debounce)
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        }
                    },
                    KeyCode::Char(c) => {
                        // Handle yield search mode
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char(c);
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        // Handle yield filter mode
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char(c);
                        } else if app.is_in_filter_mode() {
                            app.add_search_char(c);
                            // Trigger API search after character input only if in API search mode (with debounce)
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                            // Local filter mode filters immediately (no API call needed)
                        }
                    },
                    KeyCode::Enter => {
                        // Only handle Enter when EventsList panel is focused
                        if app.navigation.focused_panel == FocusedPanel::EventsList {
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
                                        let _event_slug_for_log = event_slug_clone.clone();

                                        log_info!(
                                            "Starting RTDS WebSocket for event: {}",
                                            event_slug_clone
                                        );

                                        let ws_handle = tokio::spawn(async move {
                                            match rtds_client
                                            .connect_and_listen(move |msg| {
                                                let app_state = Arc::clone(&app_state_ws);
                                                let event_slug = event_slug_for_closure.clone();

                                                log_info!(
                                                    "Received RTDS trade for event: {}",
                                                    event_slug
                                                );

                                                tokio::spawn(async move {
                                                    let mut app = app_state.lock().await;
                                                    if let Some(event_trades) =
                                                        app.trades.event_trades.get_mut(&event_slug)
                                                    {
                                                        event_trades.add_trade(&msg);
                                                        log_info!(
                                                            "Trade added to event_trades for: {}",
                                                            event_slug
                                                        );
                                                    } else {
                                                        log_warn!(
                                                            "No event_trades entry found for: {}",
                                                            event_slug
                                                        );
                                                    }
                                                });
                                            })
                                            .await
                                        {
                                            Ok(()) => {
                                                log_info!(
                                                    "RTDS WebSocket connection closed normally for event: {}",
                                                    _event_slug_for_log
                                                );
                                            },
                                            Err(_e) => {
                                                log_error!(
                                                    "RTDS WebSocket error for event {}: {}",
                                                    _event_slug_for_log,
                                                    _e
                                                );
                                            },
                                        }
                                        });

                                        app.start_watching(event_slug_clone, ws_handle);
                                    }
                                }
                            }
                        }
                    },
                    _ => {},
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
