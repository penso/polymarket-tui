// Test program to debug the log layer
// Run with: cargo run --bin test_log_layer --features tui

use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::info;
use tracing_subscriber::prelude::*;

// Import the module from the parent crate
#[path = "../tui_log_layer.rs"]
mod tui_log_layer;

#[tokio::main]
async fn main() {
    // Setup custom tracing layer to capture logs
    let logs = Arc::new(TokioMutex::new(Vec::<String>::new()));
    let log_layer = tui_log_layer::TuiLogLayer::new(Arc::clone(&logs));

    // Replace the default subscriber with one that includes our custom layer
    let _guard = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with(log_layer)
        .set_default();

    // Test various log messages
    info!("🔥 Fetching trending events...");
    info!("GET https://gamma-api.polymarket.com/events?active=true&closed=false&order=volume24hr&ascending=false&limit=50");
    info!("Found {} trending events", 42);
    info!("Triggering search for query: '{}'", "test query");

    // Give it a moment to process
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Print what we captured
    let captured_logs = logs.lock().await;
    println!("\n=== Captured Logs ({} total) ===\n", captured_logs.len());
    for (i, log) in captured_logs.iter().enumerate() {
        println!("[{}] {}", i + 1, log);
        
        // Check for double prefixes
        let info_count = log.matches("[INFO]").count();
        if info_count > 1 {
            println!("  ⚠️  WARNING: Found {} [INFO] prefixes!", info_count);
        }
        
        // Show the raw bytes to see if there are any hidden characters
        println!("  Raw bytes: {:?}", log.as_bytes());
    }
    
    println!("\n=== Analysis ===");
    let double_prefix_count = captured_logs
        .iter()
        .filter(|log| log.matches("[INFO]").count() > 1)
        .count();
    
    if double_prefix_count > 0 {
        println!("❌ Found {} logs with double [INFO] prefix", double_prefix_count);
    } else {
        println!("✅ No double [INFO] prefixes found!");
    }
}

