use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use polymarket_bot::{
    default_cache_dir, lock_mutex, GammaClient, MarketUpdateFormatter, PolymarketWebSocket,
    RTDSClient, RTDSFormatter,
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
        info!("✓ Authentication tokens found (activity subscriptions are public, auth not required)");
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

    rtds_client
        .connect_and_listen(|msg| {
            let formatted = RTDSFormatter::format_message(&msg);
            print!("{}", formatted);
        })
        .await
        .context("Failed to connect to RTDS WebSocket")?;

    Ok(())
}

async fn run_watch_event(event: String) -> Result<()> {
    let event_slug = extract_event_slug(&event);
    info!("🎯 Watching trade activity for event: {}", event_slug);
    info!("Connecting to RTDS WebSocket...");

    // Check for authentication
    let has_auth =
        env::var("api_key").is_ok() && env::var("secret").is_ok() && env::var("passphrase").is_ok();
    if has_auth {
        info!("✓ Authentication tokens found (activity subscriptions are public, auth not required)");
    } else {
        info!("ℹ️  No authentication found (activity subscriptions are public data)");
    }

    info!("Press Ctrl+C to exit");
    info!("{}", "─".repeat(80));

    let rtds_client = RTDSClient::new().with_event_slug(event_slug.clone());

    rtds_client
        .connect_and_listen(|msg| {
            let formatted = RTDSFormatter::format_message(&msg);
            print!("{}", formatted);
        })
        .await
        .context("Failed to connect to RTDS WebSocket")?;

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
        Commands::WatchEvent { event } => run_watch_event(event).await,
    }
}
