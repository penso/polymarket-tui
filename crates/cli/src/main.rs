use anyhow::{Context, Result};
use polymarket_bot::{lock_mutex, GammaClient, MarketUpdateFormatter, PolymarketWebSocket};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::info;

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

    info!("🚀 Polymarket Real-Time Monitor");
    info!("Connecting to Polymarket WebSocket...");

    let gamma_client = GammaClient::new();

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
    info!("");

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
