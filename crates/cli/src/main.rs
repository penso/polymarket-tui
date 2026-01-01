mod display_trait;

#[cfg(feature = "tui")]
mod tui;

#[cfg(feature = "tui")]
mod trending_tui;

#[cfg(all(feature = "tui", feature = "tracing"))]
mod tui_log_layer;

use {
    anyhow::{Context, Result},
    clap::{Parser, Subcommand},
    colored::Colorize,
    display_trait::TradeDisplay,
    polymarket_api::{
        ClobClient, DataClient, GammaClient, MarketUpdateFormatter, PolymarketWebSocket,
        RTDSClient, default_cache_dir, lock_mutex,
    },
    std::{
        collections::HashMap,
        env,
        path::PathBuf,
        sync::{Arc, Mutex},
    },
};

/// Macro to log info messages only when tracing feature is enabled
#[cfg(feature = "tracing")]
macro_rules! log_info {
    ($($arg:tt)*) => { tracing::info!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_info {
    ($($arg:tt)*) => {};
}

#[derive(Parser)]
#[command(name = "polymarket-cli")]
#[command(about = "Polymarket CLI tool for monitoring market data", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Monitor all active markets via WebSocket
    Monitor {
        /// Use RTDS (Real-Time Data Stream) instead of CLOB WebSocket
        /// RTDS shows actual trade activity that appears on the website
        #[arg(long)]
        rtds: bool,
        /// Event slug to filter RTDS activity (only used with --rtds)
        #[arg(long)]
        event: Option<String>,
    },
    /// Watch trade activity for a specific event using RTDS
    WatchEvent {
        /// Event URL or slug (e.g., https://polymarket.com/event/who-will-die-in-stranger-things-season-5 or who-will-die-in-stranger-things-season-5)
        #[arg(value_name = "EVENT")]
        event: String,
        /// Use TUI mode (requires --features tui)
        #[arg(long)]
        tui: bool,
    },
    /// Get orderbook for a market
    Orderbook {
        /// Market condition ID or asset ID
        #[arg(value_name = "MARKET")]
        market: String,
        /// Use asset ID instead of condition ID
        #[arg(long)]
        asset: bool,
    },
    /// Get recent trades for a market
    Trades {
        /// Market condition ID, asset ID, event ID, or event slug
        #[arg(value_name = "MARKET")]
        market: String,
        /// Limit number of trades
        #[arg(long, default_value = "10")]
        limit: usize,
        /// Use asset ID instead of condition ID
        #[arg(long)]
        asset: bool,
        /// Use event ID
        #[arg(long)]
        event_id: bool,
        /// Use event slug
        #[arg(long)]
        event_slug: bool,
    },
    /// Get event information
    Event {
        /// Event ID or slug
        #[arg(value_name = "EVENT")]
        event: String,
        /// Use event ID instead of slug
        #[arg(long)]
        id: bool,
    },
    /// Get market information
    Market {
        /// Market ID or slug
        #[arg(value_name = "MARKET")]
        market: String,
        /// Use market ID instead of slug
        #[arg(long)]
        id: bool,
    },
    /// Browse trending events in TUI (requires --features tui)
    Trending {
        /// Order by field (e.g., volume24hr, volume7d, volume30d)
        #[arg(long, default_value = "volume24hr")]
        order_by: String,
        /// Sort ascending instead of descending
        #[arg(long)]
        ascending: bool,
        /// Limit number of events
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Find high-probability markets for yield opportunities
    Yield {
        /// Minimum probability threshold (e.g., 0.99 for 99%)
        #[arg(long, default_value = "0.95")]
        min_prob: f64,
        /// Maximum number of markets to fetch
        #[arg(long, default_value = "500")]
        limit: usize,
        /// Minimum 24h volume to filter by
        #[arg(long, default_value = "0")]
        min_volume: f64,
        /// Only show events expiring within this duration (e.g., "24h", "7d", "30d")
        #[arg(long)]
        expires_in: Option<String>,
    },
}

fn extract_event_slug(event_input: &str) -> String {
    // If it's a URL, extract the slug
    if event_input.starts_with("http://") || event_input.starts_with("https://") {
        // Extract slug from URL pattern: https://polymarket.com/event/SLUG
        if let Some(slug_part) = event_input.split("/event/").nth(1) {
            // Remove query params and trailing slash if present
            slug_part
                .split('?')
                .next()
                .unwrap_or(slug_part)
                .trim_end_matches('/')
                .to_string()
        } else {
            event_input.to_string()
        }
    } else {
        // Already a slug
        event_input.to_string()
    }
}

async fn run_monitor(use_rtds: bool, event_slug: Option<String>) -> Result<()> {
    if use_rtds {
        return run_monitor_rtds(event_slug).await;
    }
    log_info!("🚀 Polymarket Real-Time Monitor");
    log_info!("Connecting to Polymarket WebSocket...");

    // Setup cache directory (configurable via POLYMARKET_CACHE_DIR env var)
    let cache_dir = env::var("POLYMARKET_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_cache_dir());

    log_info!("Using cache directory: {}", cache_dir.display());

    // Create Gamma client with file-based caching
    // Market info is cached for 24 hours (market data rarely changes)
    let mut gamma_client =
        GammaClient::with_cache(&cache_dir).context("Failed to create Gamma client with cache")?;

    // Set cache TTL to 24 hours
    gamma_client
        .set_cache_ttl(24 * 60 * 60)
        .context("Failed to set cache TTL")?;

    // Fetch active markets and get asset IDs
    log_info!("📡 Fetching active markets...");
    let asset_ids = gamma_client
        .get_all_active_asset_ids()
        .await
        .context("Failed to fetch active markets")?;

    log_info!("✓ Found {} active asset IDs", asset_ids.len());

    // Build market info cache
    log_info!("🔍 Building market info cache...");
    let market_info_cache: Arc<Mutex<HashMap<String, polymarket_api::gamma::MarketInfo>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));

    // Fetch market info for a subset (to avoid too many API calls)
    // In production, you might want to fetch all or use a more efficient approach
    let sample_size = std::cmp::min(asset_ids.len(), 50);
    let sample_asset_ids: Vec<String> = asset_ids.iter().take(sample_size).cloned().collect();

    for asset_id in &sample_asset_ids {
        if let Ok(Some(info)) = gamma_client.get_market_info_by_asset_id(asset_id).await {
            let mut cache = lock_mutex(&market_info_cache)?;
            cache.insert(asset_id.clone(), info);
        }
    }

    let _cache_len = {
        let cache = lock_mutex(&market_info_cache)?;
        cache.len()
    };
    log_info!("✓ Cached {} market info entries", _cache_len);

    // Create WebSocket client
    let mut ws_client = PolymarketWebSocket::new(asset_ids.clone());

    // Transfer cached info to WebSocket client
    {
        let cache = lock_mutex(&market_info_cache)?;
        for (asset_id, info) in cache.iter() {
            ws_client.update_market_info(asset_id.clone(), info.clone());
        }
    }

    log_info!("🔌 Connecting to WebSocket...");
    log_info!("Monitoring {} assets", asset_ids.len());
    log_info!("Press Ctrl+C to exit");
    log_info!("{}", "─".repeat(80));

    // Connect and listen
    let cache_clone = Arc::clone(&market_info_cache);

    ws_client
        .connect_and_listen(move |msg| {
            // Get market info for this message from cache
            let asset_id = match &msg {
                polymarket_api::websocket::WebSocketMessage::Orderbook(update) => {
                    Some(update.asset_id.clone())
                },
                polymarket_api::websocket::WebSocketMessage::Trade(update) => {
                    Some(update.asset_id.clone())
                },
                polymarket_api::websocket::WebSocketMessage::Order(update) => {
                    Some(update.asset_id.clone())
                },
                polymarket_api::websocket::WebSocketMessage::Price(update) => {
                    Some(update.asset_id.clone())
                },
                _ => None,
            };

            let market_info = if let Some(asset_id) = asset_id {
                // Get from cache (synchronous access)
                // Use lock_mutex helper, but handle errors gracefully in callback
                match lock_mutex(&cache_clone) {
                    Ok(cache) => cache.get(&asset_id).cloned(),
                    Err(_) => None, // If lock fails, just skip market info
                }
            } else {
                None
            };

            // Format and print
            let formatted = MarketUpdateFormatter::format_message(&msg, market_info.as_ref());
            print!("{}", formatted);
        })
        .await
        .context("WebSocket connection failed")?;

    Ok(())
}

async fn run_monitor_rtds(event_slug: Option<String>) -> Result<()> {
    log_info!("🚀 Polymarket Real-Time Monitor (RTDS)");
    log_info!("Connecting to RTDS WebSocket...");

    // Check for authentication
    let has_auth =
        env::var("api_key").is_ok() && env::var("secret").is_ok() && env::var("passphrase").is_ok();
    if has_auth {
        log_info!(
            "✓ Authentication tokens found (activity subscriptions are public, auth not required)"
        );
    } else {
        log_info!("ℹ️  No authentication found (activity subscriptions are public data)");
    }

    if let Some(ref _slug) = event_slug {
        log_info!("Filtering activity for event: {}", _slug);
    } else {
        log_info!("⚠️  No event filter specified. You may not see activity.");
        log_info!("💡 Tip: Use --event <slug> to filter by event");
    }

    log_info!("Press Ctrl+C to exit");
    log_info!("{}", "─".repeat(80));

    let mut rtds_client = RTDSClient::new();
    if let Some(slug) = event_slug {
        rtds_client = rtds_client.with_event_slug(slug);
    }

    let mut display = display_trait::SimpleDisplay {};
    rtds_client
        .connect_and_listen(|msg| {
            let _ = display.display_trade(&msg);
        })
        .await
        .context("Failed to connect to RTDS WebSocket")?;

    Ok(())
}

async fn run_watch_event(event: String, use_tui: bool) -> Result<()> {
    let event_slug = extract_event_slug(&event);
    log_info!("🎯 Watching trade activity for event: {}", event_slug);
    log_info!("Connecting to RTDS WebSocket...");

    // Check for authentication
    let has_auth =
        env::var("api_key").is_ok() && env::var("secret").is_ok() && env::var("passphrase").is_ok();
    if has_auth {
        log_info!(
            "✓ Authentication tokens found (activity subscriptions are public, auth not required)"
        );
    } else {
        log_info!("ℹ️  No authentication found (activity subscriptions are public data)");
    }

    if use_tui {
        #[cfg(feature = "tui")]
        {
            run_watch_event_tui(event_slug).await
        }
        #[cfg(not(feature = "tui"))]
        {
            anyhow::bail!("TUI mode requires building with --features tui flag");
        }
    } else {
        log_info!("Press Ctrl+C to exit");
        log_info!("{}", "─".repeat(80));

        let rtds_client = RTDSClient::new().with_event_slug(event_slug.clone());
        let mut display = display_trait::SimpleDisplay {};

        rtds_client
            .connect_and_listen(|msg| {
                let _ = display.display_trade(&msg);
            })
            .await
            .context("Failed to connect to RTDS WebSocket")?;

        Ok(())
    }
}

#[cfg(feature = "tui")]
async fn run_watch_event_tui(event_slug: String) -> Result<()> {
    use {
        crossterm::{
            event::{DisableMouseCapture, EnableMouseCapture},
            execute,
            terminal::{
                EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
            },
        },
        ratatui::{Terminal, backend::CrosstermBackend},
        std::io,
        tokio::sync::Mutex as TokioMutex,
    };

    // Fetch event data from Gamma API before starting TUI
    log_info!("Fetching event data for: {}", event_slug);
    let gamma_client = GammaClient::new();
    let event = gamma_client
        .get_event_by_slug(&event_slug)
        .await
        .context("Failed to fetch event")?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    // Create app state with event data
    let app_state = Arc::new(TokioMutex::new(tui::AppState::new_with_event(
        event_slug.clone(),
        event,
    )));

    // Trigger initial market price refresh
    let app_state_refresh = Arc::clone(&app_state);
    tokio::spawn(async move {
        tui::refresh_market_data(app_state_refresh).await;
    });

    // Clone for WebSocket task
    let app_state_ws = Arc::clone(&app_state);

    // Start WebSocket connection in background
    let rtds_client = RTDSClient::new().with_event_slug(event_slug.clone());
    let ws_handle = tokio::spawn(async move {
        let _ = rtds_client
            .connect_and_listen(|msg| {
                let app_state = Arc::clone(&app_state_ws);
                tokio::spawn(async move {
                    let mut app = app_state.lock().await;
                    app.add_trade(&msg);
                });
            })
            .await;
    });

    // Run TUI
    let tui_result = tui::run_tui(terminal, app_state).await;

    // Cleanup terminal
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);

    // Cancel WebSocket task
    ws_handle.abort();

    tui_result.context("TUI error")?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Check if we're running a TUI command
    let _is_tui_command = matches!(
        cli.command,
        Commands::Trending { .. } | Commands::WatchEvent { tui: true, .. }
    );

    // Initialize tracing subscriber conditionally
    #[cfg(feature = "tracing")]
    if !_is_tui_command {
        // For non-TUI commands, use the default fmt subscriber
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with_ansi(true)
            .init();
    }

    match cli.command {
        Commands::Monitor { rtds, event } => run_monitor(rtds, event).await,
        Commands::WatchEvent { event, tui } => run_watch_event(event, tui).await,
        Commands::Orderbook { market, asset } => run_orderbook(market, asset).await,
        Commands::Trades {
            market,
            limit,
            asset,
            event_id,
            event_slug,
        } => run_trades(market, limit, asset, event_id, event_slug).await,
        Commands::Event { event, id } => run_event(event, id).await,
        Commands::Market { market, id } => run_market(market, id).await,
        Commands::Trending {
            order_by,
            ascending,
            limit,
        } => run_trending(order_by, ascending, limit).await,
        Commands::Yield {
            min_prob,
            limit,
            min_volume,
            expires_in,
        } => run_yield(min_prob, limit, min_volume, expires_in).await,
    }
}

#[cfg(feature = "tui")]
async fn run_trending(order_by: String, ascending: bool, limit: usize) -> Result<()> {
    use {
        crossterm::{
            event::{DisableMouseCapture, EnableMouseCapture},
            execute,
            terminal::{
                EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
            },
        },
        ratatui::{Terminal, backend::CrosstermBackend},
        std::io,
        tokio::sync::Mutex as TokioMutex,
    };

    // Setup custom tracing layer to capture logs for TUI
    // IMPORTANT: Set this up BEFORE any tracing calls (including API calls)
    #[cfg(feature = "tracing")]
    let logs = Arc::new(TokioMutex::new(Vec::<String>::new()));
    #[cfg(feature = "tracing")]
    let log_layer = tui_log_layer::TuiLogLayer::new(Arc::clone(&logs));

    // Setup tracing subscriber with our custom layer
    // Use init() instead of set_default() to set it globally
    // This ensures spawned tasks can also use the dispatcher
    #[cfg(feature = "tracing")]
    {
        use tracing_subscriber::prelude::*;
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with(log_layer)
            .init();
    }

    log_info!("🔥 Fetching trending events...");

    let gamma_client = GammaClient::new();
    // For trending events, we want descending order by default (highest volume first)
    // The API's ascending=false means descending (highest first), which is what we want for trending
    let events = gamma_client
        .get_trending_events(Some(&order_by), Some(ascending), Some(limit))
        .await
        .context("Failed to fetch trending events")?;

    if events.is_empty() {
        anyhow::bail!("No trending events found");
    }

    log_info!("Found {} trending events", events.len());

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    let app_state = Arc::new(TokioMutex::new(trending_tui::TrendingAppState::new(
        events,
        order_by.clone(),
        ascending,
    )));

    // Connect logs to app state (only when tracing is enabled)
    #[cfg(feature = "tracing")]
    {
        let logs_for_app = Arc::clone(&logs);
        let app_state_for_logs = Arc::clone(&app_state);
        tokio::spawn(async move {
            let mut last_log_count = 0;
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                let logs = logs_for_app.lock().await;
                if logs.len() > last_log_count {
                    let new_logs: Vec<String> = logs[last_log_count..].to_vec();
                    last_log_count = logs.len();
                    drop(logs);

                    let mut app = app_state_for_logs.lock().await;
                    for log in new_logs {
                        // The log already has the [LEVEL] prefix from TuiLogLayer
                        // So we just pass it directly - add_log will format it
                        // Extract level for color coding
                        let level = if log.starts_with("[ERROR]") {
                            "ERROR"
                        } else if log.starts_with("[WARN]") {
                            "WARN"
                        } else if log.starts_with("[INFO]") {
                            "INFO"
                        } else if log.starts_with("[DEBUG]") {
                            "DEBUG"
                        } else {
                            "TRACE"
                        };
                        // Pass the log as-is (it already has [LEVEL] prefix)
                        // add_log will add another prefix, so we need to strip the existing one
                        let log_without_prefix = log
                            .trim_start_matches("[ERROR] ")
                            .trim_start_matches("[WARN] ")
                            .trim_start_matches("[INFO] ")
                            .trim_start_matches("[DEBUG] ")
                            .trim_start_matches("[TRACE] ")
                            .trim_start_matches("[ERROR]")
                            .trim_start_matches("[WARN]")
                            .trim_start_matches("[INFO]")
                            .trim_start_matches("[DEBUG]")
                            .trim_start_matches("[TRACE]")
                            .trim();
                        app.add_log(level, log_without_prefix.to_string());
                    }
                }
            }
        });
    }

    // Run TUI
    let result = trending_tui::run_trending_tui(terminal, app_state).await;

    // Cleanup terminal
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);

    if let Ok(Some(_event_slug)) = result {
        log_info!("Selected event: {}", _event_slug);
        log_info!(
            "You can watch this event with: polymarket-cli watch-event {}",
            _event_slug
        );
    }

    Ok(())
}

#[cfg(not(feature = "tui"))]
async fn run_trending(_order_by: String, _ascending: bool, _limit: usize) -> Result<()> {
    anyhow::bail!("Trending command requires building with --features tui flag");
}

async fn run_orderbook(market: String, use_asset: bool) -> Result<()> {
    log_info!("📊 Fetching orderbook for: {}", market);
    let clob_client = ClobClient::new();

    let orderbook = if use_asset {
        clob_client.get_orderbook_by_asset(&market).await?
    } else {
        clob_client.get_orderbook(&market).await?
    };

    log_info!("Bids (buy orders):");
    for _bid in orderbook.bids {
        log_info!("  Price: {}, Size: {}", _bid.price, _bid.size);
    }

    log_info!("Asks (sell orders):");
    for _ask in orderbook.asks {
        log_info!("  Price: {}, Size: {}", _ask.price, _ask.size);
    }

    Ok(())
}

async fn run_trades(
    market: String,
    limit: usize,
    use_asset: bool,
    use_event_id: bool,
    use_event_slug: bool,
) -> Result<()> {
    log_info!("📈 Fetching trades for: {}", market);

    if use_event_id {
        let event_id: u64 = market.parse().context("Invalid event ID")?;
        let data_client = DataClient::new();
        let trades = data_client
            .get_trades_by_event(event_id, Some(limit), None, None, None)
            .await?;
        display_trades(&trades);
    } else if use_event_slug {
        let data_client = DataClient::new();
        let trades = data_client
            .get_trades_by_event_slug(&market, Some(limit), None)
            .await?;
        display_trades(&trades);
    } else if use_asset {
        let clob_client = ClobClient::new();
        let trades = clob_client
            .get_trades_by_asset(&market, Some(limit))
            .await?;
        display_clob_trades(&trades);
    } else {
        let clob_client = ClobClient::new();
        let trades = clob_client.get_trades(&market, Some(limit)).await?;
        display_clob_trades(&trades);
    }

    Ok(())
}

async fn run_event(event: String, use_id: bool) -> Result<()> {
    log_info!("📅 Fetching event: {}", event);
    let gamma_client = GammaClient::new();

    let event_data = if use_id {
        gamma_client.get_event_by_id(&event).await?
    } else {
        gamma_client.get_event_by_slug(&event).await?
    };

    if let Some(event) = event_data {
        log_info!("Title: {}", event.title);
        log_info!("Slug: {}", event.slug);
        log_info!("Active: {}", event.active);
        log_info!("Closed: {}", event.closed);
        log_info!("Markets: {}", event.markets.len());
        for _market in event.markets.iter() {
            log_info!("  Market: {}", _market.question);
        }
    } else {
        anyhow::bail!("Event not found");
    }

    Ok(())
}

async fn run_market(market: String, use_id: bool) -> Result<()> {
    log_info!("📊 Fetching market: {}", market);
    let gamma_client = GammaClient::new();

    if use_id {
        if let Some(market_data) = gamma_client.get_market_by_id(&market).await? {
            log_info!("Question: {}", market_data.question);
            if let Some(_id) = &market_data.id {
                log_info!("Market ID: {}", _id);
            } else {
                log_info!("Market ID: (not available)");
            }
            if let Some(_token_ids) = market_data.clob_token_ids {
                log_info!("Asset IDs: {:?}", _token_ids);
            }
        } else {
            anyhow::bail!("Market not found");
        }
    } else {
        let markets = gamma_client.get_market_by_slug(&market).await?;
        if markets.is_empty() {
            anyhow::bail!("Market not found");
        }
        for market_data in markets {
            log_info!("Question: {}", market_data.question);
            if let Some(_id) = &market_data.id {
                log_info!("Market ID: {}", _id);
            } else {
                log_info!("Market ID: (not available)");
            }
            if let Some(_token_ids) = market_data.clob_token_ids {
                log_info!("Asset IDs: {:?}", _token_ids);
            }
        }
    }

    Ok(())
}

/// Parse duration string like "24h", "7d", "30d" into seconds
fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim().to_lowercase();
    if let Some(hours) = s.strip_suffix('h') {
        hours.parse::<i64>().ok().map(|h| h * 3600)
    } else if let Some(days) = s.strip_suffix('d') {
        days.parse::<i64>().ok().map(|d| d * 86400)
    } else {
        None
    }
}

async fn run_yield(
    min_prob: f64,
    limit: usize,
    min_volume: f64,
    expires_in: Option<String>,
) -> Result<()> {
    use chrono::{DateTime, Utc};

    // Parse expires_in duration if provided
    let max_end_time = expires_in
        .as_ref()
        .and_then(|s| parse_duration(s).map(|secs| Utc::now() + chrono::Duration::seconds(secs)));

    if expires_in.is_some() && max_end_time.is_none() {
        anyhow::bail!("Invalid --expires-in format. Use formats like '24h', '7d', '30d'");
    }

    log_info!(
        "🔍 Searching for markets with outcomes >= {:.1}% probability{}...",
        min_prob * 100.0,
        expires_in
            .as_ref()
            .map(|s| format!(" expiring within {}", s))
            .unwrap_or_default()
    );

    let gamma_client = GammaClient::new();

    // Fetch active markets
    let markets = gamma_client
        .get_markets(Some(true), Some(false), Some(limit))
        .await
        .context("Failed to fetch markets")?;

    log_info!("Fetched {} markets, filtering...", markets.len());

    // Structure to hold yield opportunities
    struct YieldOpportunity {
        market_name: String,
        market_status: &'static str,
        outcome: String,
        price: f64,
        est_return: f64,
        volume: f64,
        event_slug: String,
        event_title: String,
        event_status: &'static str,
        end_date: Option<DateTime<Utc>>,
    }

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

        // Filter by expiration if --expires-in is set
        if let Some(max_time) = max_end_time {
            match end_date {
                Some(end) if end <= max_time => {}, // Keep it
                _ => continue,                      // Skip if no end date or too far out
            }
        }

        // Check volume threshold (use 24hr volume as it's more reliably populated)
        let volume = market.volume_24hr.unwrap_or(0.0);
        if volume < min_volume {
            continue;
        }

        // Check each outcome price
        for (i, price_str) in market.outcome_prices.iter().enumerate() {
            if let Ok(price) = price_str.parse::<f64>()
                && price >= min_prob
            {
                let outcome = market
                    .outcomes
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("Outcome {}", i));
                let est_return = (1.0 - price) * 100.0; // Return as percentage

                // Use short name if available, otherwise market question
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
                    event_status: event.status(),
                    end_date,
                });
            }
        }
    }

    // Sort by estimated return (highest first)
    opportunities.sort_by(|a, b| b.est_return.partial_cmp(&a.est_return).unwrap());

    if opportunities.is_empty() {
        log_info!(
            "No markets found with outcomes >= {:.1}% and volume >= ${:.0}",
            min_prob * 100.0,
            min_volume
        );
        return Ok(());
    }

    log_info!(
        "\nFound {} yield opportunities (sorted by return):\n",
        opportunities.len()
    );

    // Group by event for display
    let mut current_event_slug = String::new();

    for opp in &opportunities {
        // Print event header when event changes
        if opp.event_slug != current_event_slug {
            current_event_slug = opp.event_slug.clone();
            let url = format!("https://polymarket.com/event/{}", opp.event_slug);
            let end_str = opp
                .end_date
                .map(|d| format!(" ends {}", d.format("%Y-%m-%d")))
                .unwrap_or_default();

            // Color event status
            let event_status_colored = match opp.event_status {
                "active" => opp.event_status.green(),
                "closed" => opp.event_status.red(),
                _ => opp.event_status.yellow(),
            };

            println!(
                "\n📊 {} [{}]{}",
                opp.event_title, event_status_colored, end_str
            );
            println!("   {}", url.dimmed());
            println!(
                "   {:<35} {:>6} {:>6} {:>8} {:>8} {:>10}",
                "Market", "Status", "Out", "Price", "Return", "Volume"
            );
            println!("   {}", "─".repeat(85));
        }

        let truncated_name: String = if opp.market_name.len() > 33 {
            format!("{}…", &opp.market_name[..32])
        } else {
            opp.market_name.clone()
        };

        // Color market status
        let market_status_colored = match opp.market_status {
            "open" => opp.market_status.green(),
            "closed" => opp.market_status.red(),
            "in-review" => opp.market_status.cyan(),
            _ => opp.market_status.yellow(),
        };

        let return_str = format!("{:.2}%", opp.est_return);

        println!(
            "   {:<35} {:>6} {:>6} {:>7.1}¢ {:>8} {:>9.0}$",
            truncated_name,
            market_status_colored,
            opp.outcome,
            opp.price * 100.0,
            return_str,
            opp.volume,
        );
    }

    println!("\n💡 Tip: Higher returns = higher risk. Check liquidity before trading.");

    Ok(())
}

fn display_trades(trades: &[polymarket_api::data::DataTrade]) {
    use chrono::DateTime;
    for _trade in trades {
        let _time = DateTime::from_timestamp(_trade.timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let _side = if _trade.side == "BUY" {
            "🟢 BUY"
        } else {
            "🔴 SELL"
        };
        log_info!(
            "{} | {} | {} @ ${:.4} ({} shares) | {} | {}",
            _time,
            _side,
            _trade.outcome,
            _trade.price,
            _trade.size,
            _trade.title,
            _trade.name
        );
    }
}

fn display_clob_trades(trades: &[polymarket_api::clob::Trade]) {
    use chrono::DateTime;
    for _trade in trades {
        let _time = DateTime::from_timestamp(_trade.timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let _side = if _trade.side == "BUY" {
            "🟢 BUY"
        } else {
            "🔴 SELL"
        };
        log_info!(
            "{} | {} | @ ${} ({} shares)",
            _time,
            _side,
            _trade.price,
            _trade.size
        );
    }
}
