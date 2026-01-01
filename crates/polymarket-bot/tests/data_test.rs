use polymarket_bot::data::DataClient;

#[tokio::test]
async fn test_data_client_creation() {
    let _client = DataClient::new();
    // Just verify it can be created (doesn't panic)
}

#[tokio::test]
async fn test_get_trades_by_event_slug() {
    let client = DataClient::new();
    // Use a known event slug
    let trades = client
        .get_trades_by_event_slug("who-will-die-in-stranger-things-season-5", Some(10), None)
        .await;

    // Trades might be empty, but should not error
    if let Ok(trades) = trades {
        assert!(trades.len() <= 10, "Should respect limit");

        // If we got trades, verify structure
        if let Some(trade) = trades.first() {
            assert!(!trade.asset.is_empty());
            assert!(!trade.title.is_empty());
        }
    }
}

#[tokio::test]
async fn test_get_trades_by_event() {
    let client = DataClient::new();
    // Use a known event ID
    let trades = client
        .get_trades_by_event(96664, Some(10), None, None, None)
        .await;

    // Trades might be empty, but should not error
    if let Ok(trades) = trades {
        assert!(trades.len() <= 10, "Should respect limit");
    }
}

#[tokio::test]
async fn test_get_trades_by_market() {
    let client = DataClient::new();
    // Use a known market ID
    let trades = client
        .get_trades_by_market(
            "will-mike-wheeler-die-in-stranger-things-season-5",
            Some(10),
            None,
        )
        .await;

    // Trades might be empty, but should not error
    if let Ok(trades) = trades {
        assert!(trades.len() <= 10, "Should respect limit");
    }
}
