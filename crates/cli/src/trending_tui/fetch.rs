//! Async data fetching functions for the TUI

use {
    super::{
        logging::{log_debug, log_error, log_info, log_warn},
        state::{
            self, EventFilter, OrderbookLevel, SearchMode, TrendingAppState, YieldOpportunity,
            YieldSearchResult,
        },
    },
    chrono::{DateTime, Utc},
    polymarket_api::{
        GammaClient,
        clob::{BatchTokenRequest, ClobClient, Side},
    },
    std::{collections::HashMap, sync::Arc},
    tokio::sync::Mutex as TokioMutex,
};

/// Switch to a new filter tab, using cache if available.
/// Returns `Some((order_by, limit))` if API fetch is needed, `None` if cache was used.
pub fn switch_filter_tab(
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
pub fn spawn_filter_fetch(
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
pub async fn fetch_events_for_filter(
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

/// Spawn async task to fetch API status and update app state
pub fn spawn_fetch_api_status(app_state: Arc<TokioMutex<TrendingAppState>>) {
    use polymarket_api::DataClient;

    let gamma_client = GammaClient::new();
    let data_client = DataClient::new();

    // Check Gamma API
    let app_state_gamma = Arc::clone(&app_state);
    tokio::spawn(async move {
        match gamma_client.get_status().await {
            Ok(status) => {
                let is_healthy = status == "OK" || status == "ok";
                log_info!("Gamma API status: {} (healthy={})", status, is_healthy);
                let mut app = app_state_gamma.lock().await;
                app.gamma_api_status = Some(is_healthy);
            },
            Err(e) => {
                log_error!("Gamma API status check failed: {}", e);
                let mut app = app_state_gamma.lock().await;
                app.gamma_api_status = Some(false);
            },
        }
    });

    // Check Data API
    tokio::spawn(async move {
        match data_client.get_status().await {
            Ok(status) => {
                let is_healthy = status.data == "OK" || status.data == "ok";
                log_info!("Data API status: {} (healthy={})", status.data, is_healthy);
                let mut app = app_state.lock().await;
                app.data_api_status = Some(is_healthy);
            },
            Err(e) => {
                log_error!("Data API status check failed: {}", e);
                let mut app = app_state.lock().await;
                app.data_api_status = Some(false);
            },
        }
    });
}

/// Spawn async task to fetch user profile by address and update auth state
pub fn spawn_fetch_user_profile(app_state: Arc<TokioMutex<TrendingAppState>>, address: String) {
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
pub fn spawn_fetch_portfolio(app_state: Arc<TokioMutex<TrendingAppState>>, address: String) {
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
pub fn spawn_toggle_favorite(
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
pub fn spawn_fetch_and_toggle_favorite(
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

/// Spawn async task to fetch an event by slug and add it to the cache
/// Used when an event is missing from cache (e.g., yield opportunities from markets endpoint)
pub fn spawn_fetch_event_for_cache(
    app_state: Arc<TokioMutex<TrendingAppState>>,
    event_slug: String,
) {
    let gamma_client = GammaClient::new();

    tokio::spawn(async move {
        log_info!("Fetching event for cache: {}", event_slug);

        match gamma_client.get_event_by_slug(&event_slug).await {
            Ok(Some(event)) => {
                log_info!(
                    "Cached event: {} ({} markets)",
                    event.title,
                    event.markets.len()
                );
                let mut app = app_state.lock().await;
                app.event_cache.insert(event_slug, event);
            },
            Ok(None) => {
                log_warn!("Event not found: {}", event_slug);
            },
            Err(e) => {
                log_error!("Failed to fetch event {}: {}", event_slug, e);
            },
        }
    });
}

/// Spawn async task to fetch orderbook data for a specific token ID
/// Only fetches if market_is_active is true (closed markets don't need orderbook)
pub fn spawn_fetch_orderbook(
    app_state: Arc<TokioMutex<TrendingAppState>>,
    token_id: String,
    market_is_active: bool,
) {
    // Skip fetching for inactive/closed markets
    if !market_is_active {
        return;
    }

    let clob_client = ClobClient::new();

    tokio::spawn(async move {
        log_info!("Fetching orderbook for token: {}", token_id);

        // Set loading state
        {
            let mut app = app_state.lock().await;
            app.orderbook_state.is_loading = true;
        }

        match clob_client.get_orderbook_by_asset(&token_id).await {
            Ok(orderbook) => {
                log_info!(
                    "Orderbook fetched for {}: {} bids, {} asks",
                    token_id,
                    orderbook.bids.len(),
                    orderbook.asks.len()
                );
                // Log raw API data for debugging
                if let Some(bid) = orderbook.bids.first() {
                    log_info!("Raw first bid: {} @ {}", bid.size, bid.price);
                }
                if let Some(ask) = orderbook.asks.first() {
                    log_info!("Raw first ask: {} @ {}", ask.size, ask.price);
                }

                // Convert CLOB API Orderbook to our OrderbookData
                // First, parse and sort the levels:
                // - Bids: sorted descending by price (highest/best bid first)
                // - Asks: sorted ascending by price (lowest/best ask first)
                let mut bids: Vec<OrderbookLevel> = orderbook
                    .bids
                    .iter()
                    .map(|level| {
                        let price = level.price.parse::<f64>().unwrap_or(0.0);
                        let size = level.size.parse::<f64>().unwrap_or(0.0);
                        OrderbookLevel {
                            price,
                            size,
                            total: 0.0, // Will calculate cumulative after sorting
                        }
                    })
                    .collect();
                // Sort bids descending by price (best bid = highest price first)
                bids.sort_by(|a, b| {
                    b.price
                        .partial_cmp(&a.price)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                // Calculate cumulative totals after sorting
                let mut cumulative_total = 0.0;
                for bid in &mut bids {
                    cumulative_total += bid.price * bid.size;
                    bid.total = cumulative_total;
                }

                let mut asks: Vec<OrderbookLevel> = orderbook
                    .asks
                    .iter()
                    .map(|level| {
                        let price = level.price.parse::<f64>().unwrap_or(0.0);
                        let size = level.size.parse::<f64>().unwrap_or(0.0);
                        OrderbookLevel {
                            price,
                            size,
                            total: 0.0, // Will calculate cumulative after sorting
                        }
                    })
                    .collect();
                // Sort asks ascending by price (best ask = lowest price first)
                asks.sort_by(|a, b| {
                    a.price
                        .partial_cmp(&b.price)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                // Calculate cumulative totals after sorting
                let mut cumulative_total = 0.0;
                for ask in &mut asks {
                    cumulative_total += ask.price * ask.size;
                    ask.total = cumulative_total;
                }

                // Calculate spread
                let spread = if let (Some(best_bid), Some(best_ask)) = (bids.first(), asks.first())
                {
                    Some(best_ask.price - best_bid.price)
                } else {
                    None
                };

                let orderbook_data = state::OrderbookData {
                    bids,
                    asks,
                    spread,
                    last_price: None,
                };

                // Calculate height based on data (up to 6 per side)
                let asks_count = orderbook_data.asks.len().min(6);
                let bids_count = orderbook_data.bids.len().min(6);
                let new_height = (2 + 1 + asks_count + 1 + bids_count) as u16; // borders + header + asks + spread + bids

                let mut app = app_state.lock().await;
                app.orderbook_state.orderbook = Some(orderbook_data);
                app.orderbook_state.is_loading = false;
                app.orderbook_state.last_fetch = Some(std::time::Instant::now());
                app.orderbook_state.token_id = Some(token_id);
                app.orderbook_state.last_height = new_height.max(5); // min height of 5
            },
            Err(e) => {
                log_error!("Failed to fetch orderbook for {}: {}", token_id, e);
                let mut app = app_state.lock().await;
                app.orderbook_state.is_loading = false;
            },
        }
    });
}

/// Fetch trade count for an event's markets using authenticated CLOB API
/// Returns total number of trades across all markets in the event
pub async fn fetch_event_trade_count(
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
pub async fn fetch_market_prices_batch(
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
pub async fn fetch_yield_opportunities(
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
pub fn spawn_yield_fetch(app_state: Arc<TokioMutex<TrendingAppState>>) {
    let app_state_clone = Arc::clone(&app_state);
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

        let slug_to_fetch = {
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

            // Check if the first selected event needs to be fetched
            app.yield_state
                .selected_opportunity()
                .filter(|opp| app.get_cached_event(&opp.event_slug).is_none())
                .map(|opp| opp.event_slug.clone())
        };

        // Fetch the event if not in cache (outside the lock)
        if let Some(slug) = slug_to_fetch {
            spawn_fetch_event_for_cache(app_state_clone, slug);
        }
    });
}

/// Spawn async task to fetch favorite events
pub fn spawn_fetch_favorites(app_state: Arc<TokioMutex<TrendingAppState>>) {
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
pub fn spawn_yield_search(app_state: Arc<TokioMutex<TrendingAppState>>, query: String) {
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
                    let vol_a = app
                        .get_cached_event(&a.event_slug)
                        .map(|e| {
                            e.markets
                                .iter()
                                .map(|m| m.volume_24hr.unwrap_or(0.0))
                                .sum::<f64>()
                        })
                        .unwrap_or(0.0);
                    let vol_b = app
                        .get_cached_event(&b.event_slug)
                        .map(|e| {
                            e.markets
                                .iter()
                                .map(|m| m.volume_24hr.unwrap_or(0.0))
                                .sum::<f64>()
                        })
                        .unwrap_or(0.0);
                    vol_b
                        .partial_cmp(&vol_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                },
            }
        });

        app.yield_state.set_search_results(results, query_clone);

        log_info!(
            "Yield search complete: {} results",
            app.yield_state.search_results.len()
        );
    });
}
