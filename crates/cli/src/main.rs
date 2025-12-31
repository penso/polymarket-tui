use anyhow::{Context, Result};
use colored::Colorize;
use polymarket_bot::{GammaClient, MarketUpdateFormatter, PolymarketWebSocket};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> Result<()> {
    println!("{}", "🚀 Polymarket Real-Time Monitor".bright_cyan().bold());
    println!("{}", "Connecting to Polymarket WebSocket...".dimmed());
    println!();

    let gamma_client = GammaClient::new();

    // Fetch active markets and get asset IDs
    println!("{}", "📡 Fetching active markets...".dimmed());
    let asset_ids = gamma_client
        .get_all_active_asset_ids()
        .await
        .context("Failed to fetch active markets")?;

    println!(
        "{} {} {}",
        "✓ Found".green(),
        asset_ids.len().to_string().bright_green().bold(),
        "active asset IDs".green()
    );
    println!();

    // Build market info cache
    println!("{}", "🔍 Building market info cache...".dimmed());
    let market_info_cache: Arc<Mutex<HashMap<String, polymarket_bot::gamma::MarketInfo>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));

    // Fetch market info for a subset (to avoid too many API calls)
    // In production, you might want to fetch all or use a more efficient approach
    let sample_size = std::cmp::min(asset_ids.len(), 50);
    let sample_asset_ids: Vec<String> = asset_ids.iter().take(sample_size).cloned().collect();

    for asset_id in &sample_asset_ids {
        if let Ok(Some(info)) = gamma_client.get_market_info_by_asset_id(asset_id).await {
            market_info_cache
                .lock()
                .unwrap()
                .insert(asset_id.clone(), info);
        }
    }

    println!(
        "{} {} {}",
        "✓ Cached".green(),
        market_info_cache
            .lock()
            .unwrap()
            .len()
            .to_string()
            .bright_green()
            .bold(),
        "market info entries".green()
    );
    println!();

    // Create WebSocket client
    let mut ws_client = PolymarketWebSocket::new(asset_ids.clone());

    // Transfer cached info to WebSocket client
    {
        let cache = market_info_cache.lock().unwrap();
        for (asset_id, info) in cache.iter() {
            ws_client.update_market_info(asset_id.clone(), info.clone());
        }
    }

    println!("{}", "🔌 Connecting to WebSocket...".dimmed());
    println!(
        "{}",
        format!("Monitoring {} assets", asset_ids.len()).dimmed()
    );
    println!("{}", "Press Ctrl+C to exit\n".dimmed());
    println!("{}", "─".repeat(80).dimmed());
    println!();

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
                if let Ok(cache) = cache_clone.lock() {
                    cache.get(&asset_id).cloned()
                } else {
                    None
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
