mod display_trait;

#[cfg(feature = "tui")]
mod tui;

#[cfg(feature = "tui")]
mod trending_tui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use display_trait::TradeDisplay;
use polymarket_bot::{
    default_cache_dir, lock_mutex, ClobClient, DataClient, GammaClient, MarketUpdateFormatter,
    PolymarketWebSocket, RTDSClient,
};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::info;

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
    info!("🚀 Polymarket Real-Time Monitor");
    info!("Connecting to Polymarket WebSocket...");

    // Setup cache directory (configurable via POLYMARKET_CACHE_DIR env var)
    let cache_dir = env::var("POLYMARKET_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_cache_dir());

    info!("Using cache directory: {}", cache_dir.display());

    // Create Gamma client with file-based caching
    // Market info is cached for 24 hours (market data rarely changes)
    let mut gamma_client =
        GammaClient::with_cache(&cache_dir).context("Failed to create Gamma client with cache")?;

    // Set cache TTL to 24 hours
    gamma_client
        .set_cache_ttl(24 * 60 * 60)
        .context("Failed to set cache TTL")?;

    // Fetch active markets and get asset IDs
    info!("📡 Fetching active markets...");
    let asset_ids = gamma_client
        .get_all_active_asset_ids()
        .await
        .context("Failed to fetch active markets")?;

    info!("✓ Found {} active asset IDs", asset_ids.len());

    // Build market info cache
    info!("🔍 Building market info cache...");
    let market_info_cache: Arc<Mutex<HashMap<String, polymarket_bot::gamma::MarketInfo>>> =
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

    let cache_len = {
        let cache = lock_mutex(&market_info_cache)?;
        cache.len()
    };
    info!("✓ Cached {} market info entries", cache_len);

    // Create WebSocket client
    let mut ws_client = PolymarketWebSocket::new(asset_ids.clone());

    // Transfer cached info to WebSocket client
    {
        let cache = lock_mutex(&market_info_cache)?;
        for (asset_id, info) in cache.iter() {
            ws_client.update_market_info(asset_id.clone(), info.clone());
        }
    }

    info!("🔌 Connecting to WebSocket...");
    info!("Monitoring {} assets", asset_ids.len());
    info!("Press Ctrl+C to exit");
    info!("{}", "─".repeat(80));

    // Connect and listen
    let cache_clone = Arc::clone(&market_info_cache);

    ws_client
        .connect_and_listen(move |msg| {
            // Get market info for this message from cache
            let asset_id = match &msg {
                polymarket_bot::websocket::WebSocketMessage::Orderbook(update) => {
                    Some(update.asset_id.clone())
                }
                polymarket_bot::websocket::WebSocketMessage::Trade(update) => {
                    Some(update.asset_id.clone())
                }
                polymarket_bot::websocket::WebSocketMessage::Order(update) => {
                    Some(update.asset_id.clone())
                }
                polymarket_bot::websocket::WebSocketMessage::Price(update) => {
                    Some(update.asset_id.clone())
                }
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
    info!("🚀 Polymarket Real-Time Monitor (RTDS)");
    info!("Connecting to RTDS WebSocket...");

    // Check for authentication
    let has_auth =
        env::var("api_key").is_ok() && env::var("secret").is_ok() && env::var("passphrase").is_ok();
    if has_auth {
        info!(
            "✓ Authentication tokens found (activity subscriptions are public, auth not required)"
        );
    } else {
        info!("ℹ️  No authentication found (activity subscriptions are public data)");
    }

    if let Some(ref slug) = event_slug {
        info!("Filtering activity for event: {}", slug);
    } else {
        info!("⚠️  No event filter specified. You may not see activity.");
        info!("💡 Tip: Use --event <slug> to filter by event");
    }

    info!("Press Ctrl+C to exit");
    info!("{}", "─".repeat(80));

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
    info!("🎯 Watching trade activity for event: {}", event_slug);
    info!("Connecting to RTDS WebSocket...");

    // Check for authentication
    let has_auth =
        env::var("api_key").is_ok() && env::var("secret").is_ok() && env::var("passphrase").is_ok();
    if has_auth {
        info!(
            "✓ Authentication tokens found (activity subscriptions are public, auth not required)"
        );
    } else {
        info!("ℹ️  No authentication found (activity subscriptions are public data)");
    }

    if use_tui {
        #[cfg(feature = "tui")]
        {
            return run_watch_event_tui(event_slug).await;
        }
        #[cfg(not(feature = "tui"))]
        {
            anyhow::bail!("TUI mode requires building with --features tui flag");
        }
    } else {
        info!("Press Ctrl+C to exit");
        info!("{}", "─".repeat(80));

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
    use crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::io;
    use tokio::sync::Mutex as TokioMutex;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    // Create app state
    let app_state = Arc::new(TokioMutex::new(tui::AppState::new(event_slug.clone())));

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
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_ansi(true)
        .init();

    let cli = Cli::parse();

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
    }
}

#[cfg(feature = "tui")]
async fn run_trending(order_by: String, ascending: bool, limit: usize) -> Result<()> {
    use crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::io;
    use tokio::sync::Mutex as TokioMutex;

    info!("🔥 Fetching trending events...");
    let gamma_client = GammaClient::new();
    let events = gamma_client
        .get_trending_events(Some(&order_by), Some(!ascending), Some(limit))
        .await
        .context("Failed to fetch trending events")?;

    if events.is_empty() {
        anyhow::bail!("No trending events found");
    }

    info!("Found {} trending events", events.len());

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    let app_state = Arc::new(TokioMutex::new(trending_tui::TrendingAppState::new(events)));

    // Run TUI
    let result = trending_tui::run_trending_tui(terminal, app_state).await;

    // Cleanup terminal
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);

    if let Ok(Some(event_slug)) = result {
        info!("Selected event: {}", event_slug);
        info!(
            "You can watch this event with: polymarket-cli watch-event {}",
            event_slug
        );
    }

    Ok(())
}

#[cfg(not(feature = "tui"))]
async fn run_trending(_order_by: String, _ascending: bool, _limit: usize) -> Result<()> {
    anyhow::bail!("Trending command requires building with --features tui flag");
}

async fn run_orderbook(market: String, use_asset: bool) -> Result<()> {
    info!("📊 Fetching orderbook for: {}", market);
    let clob_client = ClobClient::new();

    let orderbook = if use_asset {
        clob_client.get_orderbook_by_asset(&market).await?
    } else {
        clob_client.get_orderbook(&market).await?
    };

    info!("Bids (buy orders):");
    for bid in orderbook.bids {
        info!("  Price: {}, Size: {}", bid.price, bid.size);
    }

    info!("Asks (sell orders):");
    for ask in orderbook.asks {
        info!("  Price: {}, Size: {}", ask.price, ask.size);
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
    info!("📈 Fetching trades for: {}", market);

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
    info!("📅 Fetching event: {}", event);
    let gamma_client = GammaClient::new();

    let event_data = if use_id {
        gamma_client.get_event_by_id(&event).await?
    } else {
        gamma_client.get_event_by_slug(&event).await?
    };

    if let Some(event) = event_data {
        info!("Title: {}", event.title);
        info!("Slug: {}", event.slug);
        info!("Active: {}", event.active);
        info!("Closed: {}", event.closed);
        info!("Markets: {}", event.markets.len());
        for (i, market) in event.markets.iter().enumerate() {
            info!("  Market {}: {}", i + 1, market.question);
        }
    } else {
        anyhow::bail!("Event not found");
    }

    Ok(())
}

async fn run_market(market: String, use_id: bool) -> Result<()> {
    info!("📊 Fetching market: {}", market);
    let gamma_client = GammaClient::new();

    if use_id {
        if let Some(market_data) = gamma_client.get_market_by_id(&market).await? {
            info!("Question: {}", market_data.question);
            info!("Market ID: {}", market_data.id);
            if let Some(token_ids) = market_data.clob_token_ids {
                info!("Asset IDs: {:?}", token_ids);
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
            info!("Question: {}", market_data.question);
            info!("Market ID: {}", market_data.id);
            if let Some(token_ids) = market_data.clob_token_ids {
                info!("Asset IDs: {:?}", token_ids);
            }
        }
    }

    Ok(())
}

fn display_trades(trades: &[polymarket_bot::data::DataTrade]) {
    use chrono::DateTime;
    for trade in trades {
        let time = DateTime::from_timestamp(trade.timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        info!(
            "{} | {} | {} @ ${:.4} ({} shares) | {} | {}",
            time,
            if trade.side == "BUY" {
                "🟢 BUY".green()
            } else {
                "🔴 SELL".red()
            },
            trade.outcome,
            trade.price,
            trade.size,
            trade.title,
            trade.name
        );
    }
}

fn display_clob_trades(trades: &[polymarket_bot::clob::Trade]) {
    use chrono::DateTime;
    for trade in trades {
        let time = DateTime::from_timestamp(trade.timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        info!(
            "{} | {} | @ ${} ({} shares)",
            time,
            if trade.side == "BUY" {
                "🟢 BUY".green()
            } else {
                "🔴 SELL".red()
            },
            trade.price,
            trade.size
        );
    }
}
