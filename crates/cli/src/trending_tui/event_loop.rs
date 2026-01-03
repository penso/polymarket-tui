//! Main event loop for the trending TUI

use {
    super::{
        fetch::{
            fetch_event_trade_count, fetch_events_for_filter, fetch_market_prices_batch,
            spawn_fetch_and_toggle_favorite, spawn_fetch_api_status, spawn_fetch_event_for_cache,
            spawn_fetch_favorites, spawn_fetch_orderbook, spawn_fetch_portfolio,
            spawn_fetch_user_profile, spawn_filter_fetch, spawn_toggle_favorite, spawn_yield_fetch,
            spawn_yield_search, switch_filter_tab,
        },
        layout::{calculate_panel_areas, get_panel_at_position},
        logging::{log_error, log_info, log_warn},
        render::{self, ClickedTab, render, truncate},
        state::{
            self, EventFilter, EventTrades, FocusedPanel, MainTab, PopupType, SearchMode,
            TrendingAppState,
        },
    },
    polymarket_api::clob::ClobClient,
    ratatui::{Terminal, backend::CrosstermBackend, layout::Rect},
    std::{io, sync::Arc},
    tokio::sync::Mutex as TokioMutex,
};

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
    let mut last_status_check: tokio::time::Instant = tokio::time::Instant::now();
    // Track tab and filter changes for orderbook reset
    let mut last_main_tab: Option<MainTab> = None;
    let mut last_event_filter: Option<state::EventFilter> = None;

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

    // Fetch orderbook for the initially selected market
    {
        let app = app_state.lock().await;
        if let Some(event) = app.selected_event() {
            // Get the first non-closed market (same sorting as render_markets)
            let mut sorted_markets: Vec<_> = event.markets.iter().collect();
            sorted_markets.sort_by_key(|m| m.closed);
            let selected_idx = app
                .orderbook_state
                .selected_market_index
                .min(sorted_markets.len().saturating_sub(1));
            if let Some(market) = sorted_markets.get(selected_idx) {
                let outcome_idx = match app.orderbook_state.selected_outcome {
                    state::OrderbookOutcome::Yes => 0,
                    state::OrderbookOutcome::No => 1,
                };
                if let Some(token_id) = market
                    .clob_token_ids
                    .as_ref()
                    .and_then(|ids| ids.get(outcome_idx).cloned())
                {
                    let is_active = !market.closed;
                    drop(app);
                    spawn_fetch_orderbook(Arc::clone(&app_state), token_id, is_active);
                }
            }
        }
    }

    // Fetch API status on startup
    spawn_fetch_api_status(Arc::clone(&app_state));

    // Preload data for all filter tabs (Trending, Breaking, New)
    {
        let app = app_state.lock().await;
        let current_filter = app.event_filter;
        let limit = app.pagination.current_limit;

        // Preload the filter that isn't currently loaded
        let filters_to_preload: Vec<EventFilter> = [EventFilter::Trending, EventFilter::Breaking]
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
        // Periodically check API status (every 30 seconds)
        if last_status_check.elapsed() >= tokio::time::Duration::from_secs(30) {
            spawn_fetch_api_status(Arc::clone(&app_state));
            last_status_check = tokio::time::Instant::now();
        }

        // Check if tab or filter changed and reset orderbook if needed
        {
            let mut app = app_state.lock().await;
            let current_tab = app.main_tab;
            let current_filter = app.event_filter;
            let tab_changed = last_main_tab != Some(current_tab);
            let filter_changed = last_event_filter != Some(current_filter);

            if tab_changed || filter_changed {
                // Tab or filter changed - reset orderbook state and fetch new data
                if last_main_tab.is_some() || last_event_filter.is_some() {
                    app.orderbook_state.reset();

                    // Fetch orderbook for selected event in Events/Breaking/Favorites tabs
                    let should_fetch =
                        matches!(current_tab, MainTab::Trending | MainTab::Favorites);

                    if should_fetch {
                        // Get orderbook token from first market in sorted list (non-closed first)
                        let orderbook_info: Option<(String, bool)> =
                            if current_tab == MainTab::Favorites {
                                // For favorites, get from favorites_state
                                app.favorites_state.selected_event().and_then(|event| {
                                    let mut sorted: Vec<_> = event.markets.iter().collect();
                                    sorted.sort_by_key(|m| m.closed);
                                    sorted.first().and_then(|market| {
                                        market.clob_token_ids.as_ref().and_then(|ids| {
                                            ids.first().cloned().map(|id| (id, !market.closed))
                                        })
                                    })
                                })
                            } else {
                                // For Events/Breaking tabs
                                app.selected_event().and_then(|event| {
                                    let mut sorted: Vec<_> = event.markets.iter().collect();
                                    sorted.sort_by_key(|m| m.closed);
                                    sorted.first().and_then(|market| {
                                        market.clob_token_ids.as_ref().and_then(|ids| {
                                            ids.first().cloned().map(|id| (id, !market.closed))
                                        })
                                    })
                                })
                            };

                        if let Some((token_id, is_active)) = orderbook_info {
                            drop(app);
                            spawn_fetch_orderbook(Arc::clone(&app_state), token_id, is_active);
                        }
                    }
                }
                last_main_tab = Some(current_tab);
                last_event_filter = Some(current_filter);
            }
        }

        // Periodically refresh orderbook data (every 5 seconds) when in Events/Favorites tab
        // Skip refresh for closed/inactive markets
        {
            let app = app_state.lock().await;
            let in_orderbook_tab =
                app.main_tab == MainTab::Trending || app.main_tab == MainTab::Favorites;

            // Check if the selected market is active (not closed)
            let market_is_active = if app.main_tab == MainTab::Favorites {
                app.favorites_state.selected_event().is_some_and(|event| {
                    let mut sorted_markets: Vec<_> = event.markets.iter().collect();
                    sorted_markets.sort_by_key(|m| m.closed);
                    let idx = app
                        .orderbook_state
                        .selected_market_index
                        .min(sorted_markets.len().saturating_sub(1));
                    sorted_markets.get(idx).is_some_and(|m| !m.closed)
                })
            } else {
                app.selected_event().is_some_and(|event| {
                    let mut sorted_markets: Vec<_> = event.markets.iter().collect();
                    sorted_markets.sort_by_key(|m| m.closed);
                    let idx = app
                        .orderbook_state
                        .selected_market_index
                        .min(sorted_markets.len().saturating_sub(1));
                    sorted_markets.get(idx).is_some_and(|m| !m.closed)
                })
            };

            if in_orderbook_tab
                && market_is_active
                && !app.has_popup()
                && app.orderbook_state.needs_refresh()
                && !app.orderbook_state.is_loading
                && let Some(ref token_id) = app.orderbook_state.token_id
            {
                let token_id_clone = token_id.clone();
                drop(app);
                // market_is_active already checked above
                spawn_fetch_orderbook(Arc::clone(&app_state), token_id_clone, true);
            }
        }

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

                    // Check for orderbook title tab clicks (Yes/No toggle)
                    // Only for Trending/Breaking/Favorites tabs, not Yield
                    if app.main_tab != MainTab::Yield {
                        // Calculate orderbook area using same layout as render.rs
                        let header_height: u16 = if app.is_in_filter_mode() {
                            5
                        } else {
                            2
                        };
                        let main_area_y = header_height;
                        let _main_area_height = size
                            .height
                            .saturating_sub(header_height)
                            .saturating_sub(if app.show_logs {
                                11
                            } else {
                                3
                            });

                        // Right side starts at 40% of width
                        let right_x = size.width * 40 / 100;
                        let right_width = size.width - right_x;

                        // Orderbook is after: event details (8) + markets (7) = 15 rows from main_area_y
                        let orderbook_y = main_area_y + 8 + 7;
                        let orderbook_height: u16 = 16; // Fixed height as per calculate_orderbook_height

                        let orderbook_area =
                            Rect::new(right_x, orderbook_y, right_width, orderbook_height);

                        // Get outcome names for the selected market
                        let outcome_names: Option<(String, String)> =
                            app.selected_event().and_then(|event| {
                                let mut sorted_markets: Vec<_> = event.markets.iter().collect();
                                sorted_markets.sort_by_key(|m| m.closed);
                                let idx = app
                                    .orderbook_state
                                    .selected_market_index
                                    .min(sorted_markets.len().saturating_sub(1));
                                sorted_markets.get(idx).map(|m| {
                                    let name_0 = m
                                        .outcomes
                                        .first()
                                        .cloned()
                                        .unwrap_or_else(|| "Yes".to_string());
                                    let name_1 = m
                                        .outcomes
                                        .get(1)
                                        .cloned()
                                        .unwrap_or_else(|| "No".to_string());
                                    (name_0, name_1)
                                })
                            });

                        if let Some((name_0, name_1)) = outcome_names
                            && let Some(clicked_outcome) = render::check_orderbook_title_click(
                                mouse.column,
                                mouse.row,
                                orderbook_area,
                                &name_0,
                                &name_1,
                            )
                        {
                            if app.orderbook_state.selected_outcome != clicked_outcome {
                                app.orderbook_state.selected_outcome = clicked_outcome;
                                app.orderbook_state.orderbook = None;

                                // Fetch orderbook for the new outcome
                                if let Some(event) = app.selected_event() {
                                    let mut sorted_markets: Vec<_> = event.markets.iter().collect();
                                    sorted_markets.sort_by_key(|m| m.closed);
                                    let idx = app
                                        .orderbook_state
                                        .selected_market_index
                                        .min(sorted_markets.len().saturating_sub(1));
                                    if let Some(market) = sorted_markets.get(idx) {
                                        let is_active = !market.closed;
                                        let outcome_idx = match clicked_outcome {
                                            state::OrderbookOutcome::Yes => 0,
                                            state::OrderbookOutcome::No => 1,
                                        };
                                        if let Some(token_id) = market
                                            .clob_token_ids
                                            .as_ref()
                                            .and_then(|ids| ids.get(outcome_idx).cloned())
                                        {
                                            drop(app);
                                            spawn_fetch_orderbook(
                                                Arc::clone(&app_state),
                                                token_id,
                                                is_active,
                                            );
                                        }
                                    }
                                }
                            }
                            continue;
                        }
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
                            } else if app.main_tab == MainTab::Favorites {
                                // Favorites tab: select favorite event
                                // Account for border (1) = 1
                                let relative_y = mouse.row.saturating_sub(events_list_area.y + 1);
                                let clicked_index =
                                    app.favorites_state.scroll + relative_y as usize;
                                let favorites_len = app.favorites_state.events.len();

                                if clicked_index < favorites_len {
                                    app.favorites_state.selected_index = clicked_index;
                                    // Reset markets scroll when changing events
                                    app.scroll.markets = 0;

                                    // Fetch orderbook for the first market of the selected favorite event
                                    let orderbook_info: Option<(String, bool)> =
                                        app.favorites_state.selected_event().and_then(|event| {
                                            let mut sorted: Vec<_> = event.markets.iter().collect();
                                            sorted.sort_by_key(|m| m.closed);
                                            sorted.first().and_then(|market| {
                                                market.clob_token_ids.as_ref().and_then(|ids| {
                                                    ids.first()
                                                        .cloned()
                                                        .map(|id| (id, !market.closed))
                                                })
                                            })
                                        });
                                    app.orderbook_state.reset();
                                    if let Some((token_id, is_active)) = orderbook_info {
                                        spawn_fetch_orderbook(
                                            Arc::clone(&app_state),
                                            token_id,
                                            is_active,
                                        );
                                    }

                                    // Double-click toggles watching (same as Enter)
                                    if is_double_click
                                        && let Some(event) =
                                            app.favorites_state.selected_event().cloned()
                                    {
                                        let event_slug = event.slug.clone();
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

                                    // Fetch orderbook for the first market of the selected event
                                    // Fetch orderbook for first market (sorted, non-closed first)
                                    let orderbook_info: Option<(String, bool)> =
                                        app.selected_event().and_then(|event| {
                                            let mut sorted: Vec<_> = event.markets.iter().collect();
                                            sorted.sort_by_key(|m| m.closed);
                                            sorted.first().and_then(|market| {
                                                market.clob_token_ids.as_ref().and_then(|ids| {
                                                    ids.first()
                                                        .cloned()
                                                        .map(|id| (id, !market.closed))
                                                })
                                            })
                                        });
                                    app.orderbook_state.reset();
                                    if let Some((token_id, is_active)) = orderbook_info {
                                        spawn_fetch_orderbook(
                                            Arc::clone(&app_state),
                                            token_id,
                                            is_active,
                                        );
                                    }

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
                        // Handle click on Markets panel to select market or open trade popup
                        if panel == FocusedPanel::Markets
                            && matches!(app.main_tab, MainTab::Trending | MainTab::Favorites)
                        {
                            // Determine what was clicked: market row, Yes button, or No button
                            #[derive(Debug)]
                            enum MarketClickAction {
                                SelectMarket(usize, Option<String>, bool), /* idx, token_id for orderbook, is_active */
                                OpenTrade(String, String, String, f64), /* token_id, question, outcome, price */
                            }

                            // Get the selected event based on current tab
                            let selected_event = if app.main_tab == MainTab::Favorites {
                                app.favorites_state.selected_event().cloned()
                            } else {
                                app.selected_event().cloned()
                            };

                            let click_action: Option<MarketClickAction> =
                                if let Some(ref event) = selected_event {
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
                                    let click_x = mouse.column.saturating_sub(markets_area.x + 1);
                                    let panel_width = markets_area.width.saturating_sub(2); // borders

                                    // Sort markets same way as render_markets (non-closed first)
                                    let mut sorted_markets: Vec<_> = event.markets.iter().collect();
                                    sorted_markets.sort_by_key(|m| m.closed);

                                    if clicked_idx < sorted_markets.len() {
                                        let market = sorted_markets[clicked_idx];

                                        // For active markets, check if click is on Yes/No buttons
                                        if !market.closed {
                                            // Get prices for trade popup
                                            let yes_price = if let Some(ref token_ids) =
                                                market.clob_token_ids
                                            {
                                                token_ids.first().and_then(|asset_id| {
                                                    app.market_prices.get(asset_id).copied()
                                                })
                                            } else {
                                                None
                                            }
                                            .or_else(|| {
                                                market
                                                    .outcome_prices
                                                    .first()
                                                    .and_then(|p| p.parse::<f64>().ok())
                                            });
                                            let no_price = if let Some(ref token_ids) =
                                                market.clob_token_ids
                                            {
                                                token_ids.get(1).and_then(|asset_id| {
                                                    app.market_prices.get(asset_id).copied()
                                                })
                                            } else {
                                                None
                                            }
                                            .or_else(|| {
                                                market
                                                    .outcome_prices
                                                    .get(1)
                                                    .and_then(|p| p.parse::<f64>().ok())
                                            });

                                            // Use fixed column widths (same as render.rs)
                                            // Button column width = 17 chars each
                                            const BUTTON_COL_WIDTH: u16 = 17;

                                            // Buttons are at the right edge of the panel with fixed widths
                                            // Layout: ... [Yes button 17] [No button 17]
                                            let no_button_start =
                                                panel_width.saturating_sub(BUTTON_COL_WIDTH);
                                            let yes_button_start = no_button_start
                                                .saturating_sub(1)
                                                .saturating_sub(BUTTON_COL_WIDTH);

                                            if click_x >= no_button_start {
                                                // Clicked on No button
                                                if let Some(ref token_ids) = market.clob_token_ids {
                                                    if let Some(token_id) = token_ids.get(1) {
                                                        let outcome = market
                                                            .outcomes
                                                            .get(1)
                                                            .cloned()
                                                            .unwrap_or_else(|| "No".to_string());
                                                        Some(MarketClickAction::OpenTrade(
                                                            token_id.clone(),
                                                            market.question.clone(),
                                                            outcome,
                                                            no_price.unwrap_or(0.5),
                                                        ))
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    None
                                                }
                                            } else if click_x >= yes_button_start
                                                && click_x < no_button_start
                                            {
                                                // Clicked on Yes button
                                                if let Some(ref token_ids) = market.clob_token_ids {
                                                    if let Some(token_id) = token_ids.first() {
                                                        let outcome = market
                                                            .outcomes
                                                            .first()
                                                            .cloned()
                                                            .unwrap_or_else(|| "Yes".to_string());
                                                        Some(MarketClickAction::OpenTrade(
                                                            token_id.clone(),
                                                            market.question.clone(),
                                                            outcome,
                                                            yes_price.unwrap_or(0.5),
                                                        ))
                                                    } else {
                                                        None
                                                    }
                                                } else {
                                                    None
                                                }
                                            } else {
                                                // Clicked elsewhere on the row - select market
                                                let outcome_idx =
                                                    match app.orderbook_state.selected_outcome {
                                                        state::OrderbookOutcome::Yes => 0,
                                                        state::OrderbookOutcome::No => 1,
                                                    };
                                                let token_id = market
                                                    .clob_token_ids
                                                    .as_ref()
                                                    .and_then(|ids| ids.get(outcome_idx).cloned());
                                                Some(MarketClickAction::SelectMarket(
                                                    clicked_idx,
                                                    token_id,
                                                    true, // active market
                                                ))
                                            }
                                        } else {
                                            // Closed market - just select it
                                            let outcome_idx =
                                                match app.orderbook_state.selected_outcome {
                                                    state::OrderbookOutcome::Yes => 0,
                                                    state::OrderbookOutcome::No => 1,
                                                };
                                            let token_id = market
                                                .clob_token_ids
                                                .as_ref()
                                                .and_then(|ids| ids.get(outcome_idx).cloned());
                                            Some(MarketClickAction::SelectMarket(
                                                clicked_idx,
                                                token_id,
                                                false,
                                            )) // closed market
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                            // Handle the click action
                            match click_action {
                                Some(MarketClickAction::SelectMarket(
                                    clicked_idx,
                                    token_id,
                                    is_active,
                                )) => {
                                    if app.orderbook_state.selected_market_index != clicked_idx {
                                        app.orderbook_state.selected_market_index = clicked_idx;
                                        if let Some(token_id) = token_id {
                                            app.orderbook_state.orderbook = None;
                                            drop(app);
                                            spawn_fetch_orderbook(
                                                Arc::clone(&app_state),
                                                token_id,
                                                is_active,
                                            );
                                        }
                                    }
                                },
                                Some(MarketClickAction::OpenTrade(
                                    token_id,
                                    question,
                                    outcome,
                                    price,
                                )) => {
                                    app.open_trade_popup(
                                        token_id,
                                        question.clone(),
                                        outcome,
                                        price,
                                    );
                                    log_info!("Opening trade popup for: {}", question);
                                },
                                None => {},
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
                                    // Fetch event if not in cache
                                    if let Some(opp) = app.yield_state.selected_opportunity() {
                                        let slug = opp.event_slug.clone();
                                        if app.get_cached_event(&slug).is_none() {
                                            drop(app);
                                            spawn_fetch_event_for_cache(
                                                Arc::clone(&app_state),
                                                slug,
                                            );
                                        }
                                    }
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
                                    // Fetch event if not in cache
                                    if let Some(opp) = app.yield_state.selected_opportunity() {
                                        let slug = opp.event_slug.clone();
                                        if app.get_cached_event(&slug).is_none() {
                                            drop(app);
                                            spawn_fetch_event_for_cache(
                                                Arc::clone(&app_state),
                                                slug,
                                            );
                                        }
                                    }
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
                        KeyCode::Esc | KeyCode::Char('p') => {
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
                        // Toggle profile popup (if authenticated and not in search/filter mode)
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
                        } else if app.popup == Some(state::PopupType::UserProfile) {
                            // Close profile popup if already open
                            app.close_popup();
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
                        // Cycle sort order (or add to search/filter if in input mode)
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
                        } else if app.main_tab == MainTab::Trending
                            || app.main_tab == MainTab::Favorites
                        {
                            // Cycle sort order for Events tab
                            app.event_sort_by = app.event_sort_by.next();
                            app.sort_events();
                            app.navigation.selected_index = 0;
                            app.scroll.events_list = 0;
                            log_info!("Events sort changed to: {}", app.event_sort_by.label());
                        }
                    },
                    KeyCode::Char('S') => {
                        // Save logs to file (Shift+S) when logs panel is visible
                        if app.show_logs && !app.is_in_filter_mode() {
                            match app.logs.save_to_file() {
                                Ok(filename) => {
                                    log_info!("Logs saved to: {}", filename);
                                },
                                Err(e) => {
                                    log_error!("Failed to save logs: {}", e);
                                },
                            }
                        }
                    },
                    KeyCode::Char('t') => {
                        // Toggle orderbook Yes/No outcome (or add to search/filter if in input mode)
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            app.yield_state.add_search_char('t');
                            yield_search_debounce = Some(tokio::time::Instant::now());
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            app.yield_state.add_filter_char('t');
                        } else if app.is_in_filter_mode() {
                            app.add_search_char('t');
                            if app.search.mode == SearchMode::ApiSearch {
                                search_debounce = Some(tokio::time::Instant::now());
                            }
                        } else if matches!(app.main_tab, MainTab::Trending | MainTab::Favorites)
                            && !app.has_popup()
                        {
                            // Toggle orderbook outcome and fetch new data
                            app.orderbook_state.toggle_outcome();
                            let new_outcome = app.orderbook_state.selected_outcome;
                            log_info!(
                                "Toggled orderbook to {:?}, market_idx={}",
                                new_outcome,
                                app.orderbook_state.selected_market_index
                            );
                            // token_ids[0] = Yes, token_ids[1] = No
                            let outcome_idx = match new_outcome {
                                state::OrderbookOutcome::Yes => 0,
                                state::OrderbookOutcome::No => 1,
                            };
                            // Trigger orderbook fetch for the new outcome (use sorted markets)
                            // Get event from appropriate source based on tab
                            let orderbook_info: Option<(String, bool)> = if app.main_tab
                                == MainTab::Favorites
                            {
                                app.favorites_state.selected_event().and_then(|event| {
                                    let mut sorted_markets: Vec<_> = event.markets.iter().collect();
                                    sorted_markets.sort_by_key(|m| m.closed);
                                    let market_idx = app.orderbook_state.selected_market_index;
                                    sorted_markets.get(market_idx).and_then(|market| {
                                        log_info!(
                                            "Toggle: market={}, token_ids={:?}",
                                            market.question,
                                            market.clob_token_ids
                                        );
                                        market.clob_token_ids.as_ref().and_then(|ids| {
                                            ids.get(outcome_idx)
                                                .cloned()
                                                .map(|id| (id, !market.closed))
                                        })
                                    })
                                })
                            } else {
                                app.selected_event().and_then(|event| {
                                    let mut sorted_markets: Vec<_> = event.markets.iter().collect();
                                    sorted_markets.sort_by_key(|m| m.closed);
                                    let market_idx = app.orderbook_state.selected_market_index;
                                    sorted_markets.get(market_idx).and_then(|market| {
                                        log_info!(
                                            "Toggle: market={}, token_ids={:?}",
                                            market.question,
                                            market.clob_token_ids
                                        );
                                        market.clob_token_ids.as_ref().and_then(|ids| {
                                            ids.get(outcome_idx)
                                                .cloned()
                                                .map(|id| (id, !market.closed))
                                        })
                                    })
                                })
                            };
                            if let Some((token_id, is_active)) = orderbook_info {
                                log_info!(
                                    "Fetching orderbook for outcome_idx={}, token={}",
                                    outcome_idx,
                                    token_id
                                );
                                spawn_fetch_orderbook(Arc::clone(&app_state), token_id, is_active);
                            } else {
                                log_warn!("No token_id found for outcome_idx={}", outcome_idx);
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
                                // Fetch event if not in cache
                                if let Some(opp) = app.yield_state.selected_opportunity() {
                                    let slug = opp.event_slug.clone();
                                    if app.get_cached_event(&slug).is_none() {
                                        spawn_fetch_event_for_cache(Arc::clone(&app_state), slug);
                                    }
                                }
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

                                            // Fetch orderbook for the first market's first outcome (Yes)
                                            // Use sorted markets (non-closed first)
                                            let orderbook_info: Option<(String, bool)> = {
                                                let mut sorted: Vec<_> =
                                                    event.markets.iter().collect();
                                                sorted.sort_by_key(|m| m.closed);
                                                sorted.first().and_then(|market| {
                                                    market.clob_token_ids.as_ref().and_then(|ids| {
                                                        ids.first()
                                                            .cloned()
                                                            .map(|id| (id, !market.closed))
                                                    })
                                                })
                                            };
                                            app.orderbook_state.reset();
                                            if let Some((token_id, is_active)) = orderbook_info {
                                                spawn_fetch_orderbook(
                                                    Arc::clone(&app_state),
                                                    token_id,
                                                    is_active,
                                                );
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
                                    // Move selected market up and fetch orderbook
                                    if app.orderbook_state.selected_market_index > 0 {
                                        app.orderbook_state.selected_market_index -= 1;
                                        // Adjust scroll if needed to keep selection visible
                                        if app.orderbook_state.selected_market_index
                                            < app.scroll.markets
                                        {
                                            app.scroll.markets =
                                                app.orderbook_state.selected_market_index;
                                        }
                                        // Fetch orderbook for new selection (use sorted markets)
                                        if let Some(event) = app.selected_event() {
                                            let mut sorted_markets: Vec<_> =
                                                event.markets.iter().collect();
                                            sorted_markets.sort_by_key(|m| m.closed);
                                            let market_idx =
                                                app.orderbook_state.selected_market_index;
                                            let outcome_idx =
                                                match app.orderbook_state.selected_outcome {
                                                    state::OrderbookOutcome::Yes => 0,
                                                    state::OrderbookOutcome::No => 1,
                                                };
                                            if let Some(market) = sorted_markets.get(market_idx)
                                                && let Some(token_id) = market
                                                    .clob_token_ids
                                                    .as_ref()
                                                    .and_then(|ids| ids.get(outcome_idx).cloned())
                                            {
                                                let is_active = !market.closed;
                                                app.orderbook_state.orderbook = None;
                                                drop(app);
                                                spawn_fetch_orderbook(
                                                    Arc::clone(&app_state),
                                                    token_id,
                                                    is_active,
                                                );
                                            }
                                        }
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
                                // Fetch event if not in cache
                                if let Some(opp) = app.yield_state.selected_opportunity() {
                                    let slug = opp.event_slug.clone();
                                    if app.get_cached_event(&slug).is_none() {
                                        spawn_fetch_event_for_cache(Arc::clone(&app_state), slug);
                                    }
                                }
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

                                            // Fetch orderbook for the first market's first outcome (Yes)
                                            // Use sorted markets (non-closed first)
                                            let orderbook_info: Option<(String, bool)> = {
                                                let mut sorted: Vec<_> =
                                                    event.markets.iter().collect();
                                                sorted.sort_by_key(|m| m.closed);
                                                sorted.first().and_then(|market| {
                                                    market.clob_token_ids.as_ref().and_then(|ids| {
                                                        ids.first()
                                                            .cloned()
                                                            .map(|id| (id, !market.closed))
                                                    })
                                                })
                                            };
                                            app.orderbook_state.reset();
                                            if let Some((token_id, is_active)) = orderbook_info {
                                                spawn_fetch_orderbook(
                                                    Arc::clone(&app_state),
                                                    token_id,
                                                    is_active,
                                                );
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
                                    // Move selected market down and fetch orderbook
                                    // Extract data we need before modifying app state (use sorted markets)
                                    let market_info = app.selected_event().and_then(|event| {
                                        let mut sorted_markets: Vec<_> =
                                            event.markets.iter().collect();
                                        sorted_markets.sort_by_key(|m| m.closed);
                                        let max_index = sorted_markets.len().saturating_sub(1);
                                        let current_idx = app.orderbook_state.selected_market_index;
                                        if current_idx < max_index {
                                            let new_idx = current_idx + 1;
                                            let outcome_idx =
                                                match app.orderbook_state.selected_outcome {
                                                    state::OrderbookOutcome::Yes => 0,
                                                    state::OrderbookOutcome::No => 1,
                                                };
                                            let token_and_active =
                                                sorted_markets.get(new_idx).and_then(|market| {
                                                    market.clob_token_ids.as_ref().and_then(|ids| {
                                                        ids.get(outcome_idx)
                                                            .cloned()
                                                            .map(|id| (id, !market.closed))
                                                    })
                                                });
                                            Some((new_idx, token_and_active))
                                        } else {
                                            None
                                        }
                                    });

                                    if let Some((new_idx, token_and_active)) = market_info {
                                        app.orderbook_state.selected_market_index = new_idx;
                                        // Adjust scroll if needed to keep selection visible
                                        let visible_height: usize = 5; // Markets panel height
                                        if new_idx >= app.scroll.markets + visible_height {
                                            app.scroll.markets =
                                                new_idx.saturating_sub(visible_height - 1);
                                        }
                                        // Fetch orderbook for new selection
                                        if let Some((token_id, is_active)) = token_and_active {
                                            app.orderbook_state.orderbook = None;
                                            drop(app);
                                            spawn_fetch_orderbook(
                                                Arc::clone(&app_state),
                                                token_id,
                                                is_active,
                                            );
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
                        // Handle yield search/filter mode first
                        if app.main_tab == MainTab::Yield && app.yield_state.is_searching {
                            // Hide search input but keep results
                            app.yield_state.hide_search_input();
                            log_info!("Hidden yield search input, keeping results");
                            continue;
                        } else if app.main_tab == MainTab::Yield && app.yield_state.is_filtering {
                            // Exit filter mode but keep filter applied
                            app.yield_state.exit_filter_mode();
                            log_info!("Exited yield filter mode");
                            continue;
                        }

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
