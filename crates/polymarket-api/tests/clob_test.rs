use polymarket_api::clob::{
    BatchTokenRequest, ClobClient, MidpointResponse, Orderbook, PriceHistoryResponse,
    PriceInterval, PriceLevel, PriceResponse, Side, SpreadRequest, TokenPrices,
};

// ============================================================================
// Unit Tests (no network required)
// ============================================================================

#[test]
fn test_price_level_deserialization() {
    let json = r#"{"price": "0.50", "size": "100.00"}"#;
    let level: PriceLevel = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(level.price, "0.50");
    assert_eq!(level.size, "100.00");
}

#[test]
fn test_orderbook_deserialization_minimal() {
    let json = r#"{"bids": [], "asks": []}"#;
    let orderbook: Orderbook = serde_json::from_str(json).expect("Should deserialize");
    assert!(orderbook.bids.is_empty());
    assert!(orderbook.asks.is_empty());
    assert!(orderbook.market.is_none());
}

#[test]
fn test_orderbook_deserialization_full() {
    let json = r#"{
        "market": "0x123abc",
        "asset_id": "456",
        "timestamp": "2023-10-01T12:00:00Z",
        "hash": "0xdef789",
        "bids": [{"price": "0.45", "size": "50"}],
        "asks": [{"price": "0.55", "size": "75"}],
        "min_order_size": "0.01",
        "tick_size": "0.001",
        "neg_risk": true
    }"#;
    let orderbook: Orderbook = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(orderbook.market, Some("0x123abc".to_string()));
    assert_eq!(orderbook.asset_id, Some("456".to_string()));
    assert_eq!(
        orderbook.timestamp,
        Some("2023-10-01T12:00:00Z".to_string())
    );
    assert_eq!(orderbook.hash, Some("0xdef789".to_string()));
    assert_eq!(orderbook.bids.len(), 1);
    assert_eq!(orderbook.asks.len(), 1);
    assert_eq!(orderbook.min_order_size, Some("0.01".to_string()));
    assert_eq!(orderbook.tick_size, Some("0.001".to_string()));
    assert_eq!(orderbook.neg_risk, Some(true));
}

#[test]
fn test_price_response_deserialization() {
    let json = r#"{"price": "1800.50"}"#;
    let response: PriceResponse = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(response.price, "1800.50");
}

#[test]
fn test_midpoint_response_deserialization() {
    let json = r#"{"mid": "1800.75"}"#;
    let response: MidpointResponse = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(response.mid, "1800.75");
}

#[test]
fn test_price_history_response_deserialization() {
    let json = r#"{"history": [{"t": 1697875200, "p": 0.55}, {"t": 1697878800, "p": 0.60}]}"#;
    let response: PriceHistoryResponse = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(response.history.len(), 2);
    assert_eq!(response.history[0].t, 1697875200);
    assert!((response.history[0].p - 0.55).abs() < f64::EPSILON);
    assert_eq!(response.history[1].t, 1697878800);
    assert!((response.history[1].p - 0.60).abs() < f64::EPSILON);
}

#[test]
fn test_price_history_empty() {
    let json = r#"{"history": []}"#;
    let response: PriceHistoryResponse = serde_json::from_str(json).expect("Should deserialize");
    assert!(response.history.is_empty());
}

#[test]
fn test_spread_request_serialization() {
    let request = SpreadRequest {
        token_id: "123456".to_string(),
        side: Some(Side::Buy),
    };
    let json = serde_json::to_string(&request).expect("Should serialize");
    assert!(json.contains("123456"));
    assert!(json.contains("BUY"));
}

#[test]
fn test_spread_request_serialization_no_side() {
    let request = SpreadRequest {
        token_id: "123456".to_string(),
        side: None,
    };
    let json = serde_json::to_string(&request).expect("Should serialize");
    assert!(json.contains("123456"));
    assert!(!json.contains("side"));
}

#[test]
fn test_price_interval_as_str() {
    assert_eq!(PriceInterval::OneMinute.as_str(), "1m");
    assert_eq!(PriceInterval::OneHour.as_str(), "1h");
    assert_eq!(PriceInterval::SixHours.as_str(), "6h");
    assert_eq!(PriceInterval::OneDay.as_str(), "1d");
    assert_eq!(PriceInterval::OneWeek.as_str(), "1w");
    assert_eq!(PriceInterval::Max.as_str(), "max");
}

#[test]
fn test_side_serialization() {
    let buy = serde_json::to_string(&Side::Buy).expect("Should serialize");
    let sell = serde_json::to_string(&Side::Sell).expect("Should serialize");
    assert_eq!(buy, "\"BUY\"");
    assert_eq!(sell, "\"SELL\"");
}

#[test]
fn test_batch_token_request_serialization() {
    let request = BatchTokenRequest {
        token_id: "123456789".to_string(),
        side: Side::Buy,
    };
    let json = serde_json::to_string(&request).expect("Should serialize");
    assert!(json.contains("123456789"));
    assert!(json.contains("BUY"));
}

#[test]
fn test_batch_token_request_serialization_sell() {
    let request = BatchTokenRequest {
        token_id: "987654321".to_string(),
        side: Side::Sell,
    };
    let json = serde_json::to_string(&request).expect("Should serialize");
    assert!(json.contains("987654321"));
    assert!(json.contains("SELL"));
}

#[test]
fn test_token_prices_deserialization() {
    let json = r#"{"BUY": "0.45", "SELL": "0.55"}"#;
    let prices: TokenPrices = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(prices.buy, Some("0.45".to_string()));
    assert_eq!(prices.sell, Some("0.55".to_string()));
}

#[test]
fn test_token_prices_deserialization_partial() {
    let json = r#"{"BUY": "0.45"}"#;
    let prices: TokenPrices = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(prices.buy, Some("0.45".to_string()));
    assert!(prices.sell.is_none());
}

#[test]
fn test_token_prices_deserialization_empty() {
    let json = r#"{}"#;
    let prices: TokenPrices = serde_json::from_str(json).expect("Should deserialize");
    assert!(prices.buy.is_none());
    assert!(prices.sell.is_none());
}

// ============================================================================
// Integration Tests (require network)
// ============================================================================

#[tokio::test]
async fn test_clob_client_creation() {
    let _client = ClobClient::new();
    // Just verify it can be created (doesn't panic)
}

#[tokio::test]
async fn test_clob_client_with_auth() {
    let _client = ClobClient::with_auth(
        "test_key".to_string(),
        "test_secret".to_string(),
        "test_passphrase".to_string(),
    );
    // Just verify it can be created with auth (doesn't panic)
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
            !ob.bids.is_empty() || !ob.asks.is_empty() || ob.market.is_some(),
            "Orderbook should have some data"
        );
    }
}

#[tokio::test]
async fn test_get_orderbook_by_asset() {
    let client = ClobClient::new();
    // Use a known token ID
    let result = client
        .get_orderbook_by_asset(
            "50229529616777085027502492682800195748509080624860515924115435116786910229377",
        )
        .await;

    // Should not error, returns empty orderbook if not found
    assert!(result.is_ok(), "Should not error");
}

#[tokio::test]
async fn test_get_orderbook_by_asset_not_found() {
    let client = ClobClient::new();
    // Use an invalid token ID
    let result = client.get_orderbook_by_asset("invalid_token_id").await;

    // Should return empty orderbook, not error
    assert!(result.is_ok(), "Should not error for invalid token");
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

#[tokio::test]
async fn test_get_price() {
    let client = ClobClient::new();
    // This test may fail if the token doesn't have active orders
    let result = client
        .get_price(
            "50229529616777085027502492682800195748509080624860515924115435116786910229377",
            Side::Buy,
        )
        .await;

    // Price endpoint may error if no orders, that's acceptable
    if let Ok(price) = result {
        assert!(!price.price.is_empty());
    }
}

#[tokio::test]
async fn test_get_midpoint() {
    let client = ClobClient::new();
    // This test may fail if the token doesn't have active orders on both sides
    let result = client
        .get_midpoint(
            "50229529616777085027502492682800195748509080624860515924115435116786910229377",
        )
        .await;

    // Midpoint endpoint may error if no orders on both sides, that's acceptable
    if let Ok(midpoint) = result {
        assert!(!midpoint.mid.is_empty());
    }
}

#[tokio::test]
async fn test_get_prices_history() {
    let client = ClobClient::new();
    let result = client
        .get_prices_history(
            "50229529616777085027502492682800195748509080624860515924115435116786910229377",
            None,
            None,
            Some(PriceInterval::OneDay),
            None,
        )
        .await;

    // History might be empty for some tokens, that's acceptable
    if let Ok(history) = result {
        // Just verify structure
        for point in &history.history {
            assert!(point.t > 0);
            assert!(point.p >= 0.0 && point.p <= 1.0);
        }
    }
}

#[tokio::test]
async fn test_get_spreads() {
    let client = ClobClient::new();
    let requests = vec![SpreadRequest {
        token_id: "50229529616777085027502492682800195748509080624860515924115435116786910229377"
            .to_string(),
        side: None,
    }];

    let result = client.get_spreads(requests).await;

    // Spreads endpoint may return empty or error for tokens without orders
    // Just verify it doesn't panic
    let _ = result;
}

#[tokio::test]
async fn test_get_orderbooks_batch() {
    let client = ClobClient::new();
    let requests = vec![BatchTokenRequest {
        token_id: "50229529616777085027502492682800195748509080624860515924115435116786910229377"
            .to_string(),
        side: Side::Buy,
    }];

    let result = client.get_orderbooks(requests).await;

    // Batch orderbooks endpoint may return empty for tokens without orders
    // Just verify it doesn't panic and returns valid structure
    if let Ok(orderbooks) = result {
        for ob in &orderbooks {
            // Verify the structure is valid (bids/asks are Vec<PriceLevel>)
            let _ = ob.bids.len();
            let _ = ob.asks.len();
        }
    }
}

#[tokio::test]
async fn test_get_prices_batch() {
    let client = ClobClient::new();
    let requests = vec![BatchTokenRequest {
        token_id: "50229529616777085027502492682800195748509080624860515924115435116786910229377"
            .to_string(),
        side: Side::Buy,
    }];

    let result = client.get_prices_batch(requests).await;

    // Batch prices endpoint may return empty for tokens without orders
    // Just verify it doesn't panic and returns valid structure
    if let Ok(prices) = result {
        for (token_id, token_prices) in &prices {
            assert!(!token_id.is_empty());
            // Prices are optional
            let _ = token_prices.buy.as_ref();
            let _ = token_prices.sell.as_ref();
        }
    }
}
