//! TUI for browsing trending events with live trade monitoring

mod keys;
mod render;
mod state;

use {
    render::{render, truncate},
    state::{EventTrades, FocusedPanel, SearchMode},
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
    polymarket_api::clob::ClobClient,
    ratatui::{Terminal, backend::CrosstermBackend},
    std::{collections::HashMap, io, sync::Arc},
    tokio::sync::Mutex as TokioMutex,
};

pub async fn run_trending_tui(
    mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
    app_state: Arc<TokioMutex<TrendingAppState>>,
) -> anyhow::Result<Option<String>> {
    use {
        crossterm::event::{self, Event, KeyCode, KeyEventKind},
        polymarket_api::{GammaClient, RTDSClient},
    };

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

        {
            let mut app = app_state.lock().await;
            terminal.draw(|f| {
                render(f, &mut app);
            })?;
        }

        if crossterm::event::poll(std::time::Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            let mut app = app_state.lock().await;
            match key.code {
                KeyCode::Char('q') => {
                    if app.is_in_filter_mode() {
                        app.exit_search_mode();
                    } else {
                        app.should_quit = true;
                        break;
                    }
                },
                KeyCode::Esc => {
                    if app.is_in_filter_mode() {
                        app.exit_search_mode();
                    } else {
                        app.should_quit = true;
                        break;
                    }
                },
                KeyCode::Char('/') => {
                    // Search is only available when EventsList panel is focused
                    if !app.is_in_filter_mode()
                        && app.navigation.focused_panel == FocusedPanel::EventsList
                    {
                        app.enter_search_mode();
                    }
                },
                KeyCode::Char('f') => {
                    // Local filter is only available when EventsList panel is focused
                    if !app.is_in_filter_mode()
                        && app.navigation.focused_panel == FocusedPanel::EventsList
                    {
                        app.enter_local_filter_mode();
                    }
                },
                KeyCode::Char('r') => {
                    // Refresh market prices - only available when Markets panel is focused
                    if !app.is_in_filter_mode()
                        && app.navigation.focused_panel == FocusedPanel::Markets
                        && let Some(event) = app.selected_event()
                    {
                        log_info!("Refreshing market prices for event: {}", event.slug);
                        let app_state_clone = Arc::clone(&app_state);
                        let clob_client = ClobClient::new();
                        let markets_clone: Vec<_> = event
                            .markets
                            .iter()
                            .map(|m| m.clob_token_ids.clone())
                            .collect();

                        tokio::spawn(async move {
                            let mut prices: HashMap<String, f64> = HashMap::new();
                            for token_ids in markets_clone.into_iter().flatten() {
                                for asset_id in token_ids {
                                    match clob_client.get_orderbook_by_asset(&asset_id).await {
                                        Ok(orderbook) => {
                                            // Find the best (lowest) ask price
                                            let best_price = orderbook
                                                .asks
                                                .iter()
                                                .filter_map(|ask| ask.price.parse::<f64>().ok())
                                                .min_by(|a, b| a.partial_cmp(b).unwrap());
                                            if let Some(price) = best_price {
                                                prices.insert(asset_id.clone(), price);
                                            }
                                        },
                                        Err(_e) => {
                                            log_debug!(
                                                "Failed to fetch orderbook for asset {}: {}",
                                                asset_id,
                                                _e
                                            );
                                        },
                                    }
                                }
                            }

                            let mut app = app_state_clone.lock().await;
                            app.market_prices.extend(prices);
                            log_info!("Market prices refreshed");
                        });
                    }
                },
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
                },
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

                            log_info!(
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
                                        log_info!(
                                            "Fetched {} events for {} filter",
                                            new_events.len(),
                                            order_by
                                        );
                                        let mut app = app_state_clone.lock().await;
                                        app.events = new_events;
                                        app.pagination.is_fetching_more = false;
                                        app.navigation.selected_index = 0;
                                        app.scroll.events_list = 0;
                                    },
                                    Err(_e) => {
                                        log_error!("Failed to fetch events: {}", _e);
                                        let mut app = app_state_clone.lock().await;
                                        app.pagination.is_fetching_more = false;
                                    },
                                }
                            });
                        }
                    }
                },
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

                            log_info!(
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
                                        log_info!(
                                            "Fetched {} events for {} filter",
                                            new_events.len(),
                                            order_by
                                        );
                                        let mut app = app_state_clone.lock().await;
                                        app.events = new_events;
                                        app.pagination.is_fetching_more = false;
                                        app.navigation.selected_index = 0;
                                        app.scroll.events_list = 0;
                                    },
                                    Err(_e) => {
                                        log_error!("Failed to fetch events: {}", _e);
                                        let mut app = app_state_clone.lock().await;
                                        app.pagination.is_fetching_more = false;
                                    },
                                }
                            });
                        }
                    }
                },
                KeyCode::Up => {
                    if !app.is_in_filter_mode() {
                        match app.navigation.focused_panel {
                            FocusedPanel::Header => {
                                // Header doesn't scroll, but we can allow it for consistency
                            },
                            FocusedPanel::EventsList => {
                                app.move_up();
                                // Fetch market prices when event selection changes
                                if let Some(event) = app.selected_event() {
                                    let current_slug = event.slug.clone();
                                    if last_selected_event_slug.as_ref() != Some(&current_slug) {
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
                                            for token_ids in markets_clone.into_iter().flatten() {
                                                for asset_id in token_ids {
                                                    match clob_client
                                                        .get_orderbook_by_asset(&asset_id)
                                                        .await
                                                    {
                                                        Ok(orderbook) => {
                                                            // Find the best (lowest) ask price
                                                            let best_price = orderbook
                                                                .asks
                                                                .iter()
                                                                .filter_map(|ask| {
                                                                    ask.price.parse::<f64>().ok()
                                                                })
                                                                .min_by(|a, b| {
                                                                    a.partial_cmp(b).unwrap()
                                                                });
                                                            if let Some(price) = best_price {
                                                                prices.insert(
                                                                    asset_id.clone(),
                                                                    price,
                                                                );
                                                                log_debug!(
                                                                    "Fetched price for asset {}: ${:.3}",
                                                                    asset_id,
                                                                    price
                                                                );
                                                            }
                                                        },
                                                        Err(_e) => {
                                                            // Only log as debug to reduce noise - empty orderbooks are common
                                                            log_debug!(
                                                                "Failed to fetch orderbook for asset {}: {}",
                                                                asset_id,
                                                                _e
                                                            );
                                                        },
                                                    }
                                                }
                                            }

                                            let mut app = app_state_clone.lock().await;
                                            app.market_prices.extend(prices);
                                        });
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
                        match app.navigation.focused_panel {
                            FocusedPanel::Header => {
                                // Header doesn't scroll, but we can allow it for consistency
                            },
                            FocusedPanel::EventsList => {
                                app.move_down();
                                // Fetch market prices when event selection changes
                                if let Some(event) = app.selected_event() {
                                    let current_slug = event.slug.clone();
                                    if last_selected_event_slug.as_ref() != Some(&current_slug) {
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
                                            for token_ids in markets_clone.into_iter().flatten() {
                                                for asset_id in token_ids {
                                                    match clob_client
                                                        .get_orderbook_by_asset(&asset_id)
                                                        .await
                                                    {
                                                        Ok(orderbook) => {
                                                            // Find the best (lowest) ask price
                                                            let best_price = orderbook
                                                                .asks
                                                                .iter()
                                                                .filter_map(|ask| {
                                                                    ask.price.parse::<f64>().ok()
                                                                })
                                                                .min_by(|a, b| {
                                                                    a.partial_cmp(b).unwrap()
                                                                });
                                                            if let Some(price) = best_price {
                                                                prices.insert(
                                                                    asset_id.clone(),
                                                                    price,
                                                                );
                                                                log_debug!(
                                                                    "Fetched price for asset {}: ${:.3}",
                                                                    asset_id,
                                                                    price
                                                                );
                                                            }
                                                        },
                                                        Err(_e) => {
                                                            // Only log as debug to reduce noise - empty orderbooks are common
                                                            log_debug!(
                                                                "Failed to fetch orderbook for asset {}: {}",
                                                                asset_id,
                                                                _e
                                                            );
                                                        },
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
                                    log_info!(
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

                                                new_events
                                                    .retain(|e| !existing_slugs.contains(&e.slug));

                                                if !new_events.is_empty() {
                                                    log_info!(
                                                        "Fetched {} new trending events",
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
                                                log_error!("Failed to fetch more events: {}", _e);
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
                                if app.scroll.trades < trades_len.saturating_sub(visible_height) {
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
                    if app.is_in_filter_mode() {
                        app.delete_search_char();
                        // Trigger API search after backspace only if in API search mode (with debounce)
                        if app.search.mode == SearchMode::ApiSearch {
                            search_debounce = Some(tokio::time::Instant::now());
                        }
                    }
                },
                KeyCode::Char(c) => {
                    if app.is_in_filter_mode() {
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

                                    let rtds_client =
                                        RTDSClient::new().with_event_slug(event_slug_clone.clone());
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
