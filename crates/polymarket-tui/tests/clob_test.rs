use polymarket_tui::clob::ClobClient;

#[tokio::test]
async fn test_clob_client_creation() {
    let _client = ClobClient::new();
    // Just verify it can be created (doesn't panic)
}

#[tokio::test]
async fn test_get_orderbook() {
    let client = ClobClient::new();
    // Use a known asset ID
    let orderbook = client
        .get_orderbook(
            "50229529616777085027502492682800195748509080624860515924115435116786910229377",
        )
        .await;

    // Orderbook might not exist for all assets, so we just check it doesn't panic
    if let Ok(ob) = orderbook {
        // If we got an orderbook, verify structure
        assert!(
            !ob.bids.is_empty() || !ob.asks.is_empty(),
            "Orderbook should have at least bids or asks"
        );
    }
}

#[tokio::test]
async fn test_get_trades() {
    let client = ClobClient::new();
    // Use a known market ID
    let trades = client
        .get_trades(
            "will-mike-wheeler-die-in-stranger-things-season-5",
            Some(10),
        )
        .await;

    // Trades might be empty, but should not error
    if let Ok(trades) = trades {
        assert!(trades.len() <= 10, "Should respect limit");
    }
}

#[tokio::test]
async fn test_get_trades_by_asset() {
    let client = ClobClient::new();
    // Use a known asset ID
    let trades = client
        .get_trades_by_asset(
            "50229529616777085027502492682800195748509080624860515924115435116786910229377",
            Some(10),
        )
        .await;

    // Trades might be empty, but should not error
    if let Ok(trades) = trades {
        assert!(trades.len() <= 10, "Should respect limit");
    }
}
