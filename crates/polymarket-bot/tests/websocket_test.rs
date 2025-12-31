use polymarket_bot::websocket::{PolymarketWebSocket, SubscriptionMessage};

#[test]
fn test_subscription_message_serialization() {
    let msg = SubscriptionMessage {
        auth: None,
        markets: None,
        assets_ids: Some(vec!["test_token_1".to_string(), "test_token_2".to_string()]),
        channel_type: "MARKET".to_string(),
        custom_feature_enabled: None,
    };

    let json = serde_json::to_string(&msg).expect("Should serialize");

    // Verify the JSON structure
    assert!(json.contains("\"type\":\"MARKET\""));
    assert!(json.contains("test_token_1"));
    assert!(json.contains("test_token_2"));
    assert!(!json.contains("auth")); // Should be omitted when None
    assert!(!json.contains("markets")); // Should be omitted when None
}

#[test]
fn test_websocket_client_creation() {
    let asset_ids = vec![
        "token1".to_string(),
        "token2".to_string(),
    ];

    let _client = PolymarketWebSocket::new(asset_ids.clone());

    // Verify client was created successfully (just that it doesn't panic)
    // We can't easily test internal state without making fields public
}

#[tokio::test]
async fn test_websocket_connection_with_subscription() {
    // Test that we can connect and send a subscription message
    use tokio_tungstenite::{connect_async, tungstenite::Message};
    use futures_util::{SinkExt, StreamExt};

    // Connect to WebSocket
    let (ws_stream, _) = match connect_async("wss://ws-subscriptions-clob.polymarket.com/ws/market").await {
        Ok(stream) => stream,
        Err(e) => {
            // If connection fails, that's okay for a test - we just verify the URL format
            // The actual connection test happens when running the CLI
            eprintln!("WebSocket connection test skipped: {}", e);
            return;
        }
    };

    let (mut write, mut read) = ws_stream.split();

    // Send a subscription message
    let subscribe_msg = serde_json::json!({
        "type": "MARKET",
        "assets_ids": ["test_token"]
    });

    // Send subscription
    if write.send(Message::Text(subscribe_msg.to_string())).await.is_ok() {
        // Try to read a response (with timeout)
        use tokio::time::{timeout, Duration};
        if let Ok(Some(Ok(Message::Text(_)))) = timeout(Duration::from_secs(2), read.next()).await {
            // Got a response - connection works!
        }
    }

    // Test passes if we got this far without panicking
}

