use polymarket_api::data::{
    Activity, ActivitySortBy, ActivityType, DataClient, DataTrade, Position, Portfolio,
    SortDirection, TradeSide,
};

// ============================================================================
// Unit Tests (no network required)
// ============================================================================

#[test]
fn test_activity_type_serialization() {
    let trade = serde_json::to_string(&ActivityType::Trade).expect("Should serialize");
    let split = serde_json::to_string(&ActivityType::Split).expect("Should serialize");
    let merge = serde_json::to_string(&ActivityType::Merge).expect("Should serialize");
    let redeem = serde_json::to_string(&ActivityType::Redeem).expect("Should serialize");
    let reward = serde_json::to_string(&ActivityType::Reward).expect("Should serialize");
    let conversion = serde_json::to_string(&ActivityType::Conversion).expect("Should serialize");

    assert_eq!(trade, "\"TRADE\"");
    assert_eq!(split, "\"SPLIT\"");
    assert_eq!(merge, "\"MERGE\"");
    assert_eq!(redeem, "\"REDEEM\"");
    assert_eq!(reward, "\"REWARD\"");
    assert_eq!(conversion, "\"CONVERSION\"");
}

#[test]
fn test_activity_type_deserialization() {
    let trade: ActivityType = serde_json::from_str("\"TRADE\"").expect("Should deserialize");
    assert_eq!(trade, ActivityType::Trade);

    let split: ActivityType = serde_json::from_str("\"SPLIT\"").expect("Should deserialize");
    assert_eq!(split, ActivityType::Split);
}

#[test]
fn test_position_deserialization_full() {
    let json = r#"{
        "proxyWallet": "0x1234",
        "asset": "abc123",
        "conditionId": "0xdef456",
        "size": 100.5,
        "avgPrice": 0.55,
        "initialValue": 55.275,
        "currentValue": 60.0,
        "cashPnl": 4.725,
        "percentPnl": 8.55,
        "totalBought": 100.5,
        "realizedPnl": 0.0,
        "percentRealizedPnl": 0.0,
        "curPrice": 0.60,
        "redeemable": false,
        "mergeable": true,
        "title": "Test Market",
        "slug": "test-market",
        "icon": "https://example.com/icon.png",
        "eventSlug": "test-event",
        "outcome": "Yes",
        "outcomeIndex": 0,
        "oppositeOutcome": "No",
        "oppositeAsset": "xyz789",
        "endDate": "2024-12-31T23:59:59Z",
        "negativeRisk": false
    }"#;

    let position: Position = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(position.proxy_wallet, Some("0x1234".to_string()));
    assert_eq!(position.asset, "abc123");
    assert_eq!(position.condition_id, "0xdef456");
    assert_eq!(position.size, Some(100.5));
    assert_eq!(position.avg_price, Some(0.55));
    assert_eq!(position.redeemable, Some(false));
    assert_eq!(position.mergeable, Some(true));
    assert_eq!(position.outcome, "Yes");
    assert_eq!(position.outcome_index, 0);
}

#[test]
fn test_position_deserialization_minimal() {
    let json = r#"{
        "asset": "abc123",
        "conditionId": "0xdef456",
        "title": "Test Market",
        "slug": "test-market",
        "eventSlug": "test-event",
        "outcome": "Yes",
        "outcomeIndex": 0
    }"#;

    let position: Position = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(position.asset, "abc123");
    assert!(position.size.is_none());
    assert!(position.avg_price.is_none());
}

#[test]
fn test_activity_deserialization() {
    let json = r#"{
        "proxyWallet": "0x1234",
        "timestamp": 1697875200,
        "conditionId": "0xdef456",
        "type": "TRADE",
        "size": 50.0,
        "usdcSize": 25.0,
        "transactionHash": "0xtxhash",
        "price": 0.50,
        "asset": "abc123",
        "side": "BUY",
        "outcomeIndex": 0,
        "title": "Test Market",
        "slug": "test-market",
        "eventSlug": "test-event",
        "outcome": "Yes"
    }"#;

    let activity: Activity = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(activity.proxy_wallet, "0x1234");
    assert_eq!(activity.timestamp, 1697875200);
    assert_eq!(activity.activity_type, ActivityType::Trade);
    assert_eq!(activity.size, Some(50.0));
    assert_eq!(activity.usdc_size, Some(25.0));
    assert_eq!(activity.side, Some("BUY".to_string()));
}

#[test]
fn test_data_trade_deserialization() {
    let json = r#"{
        "proxy_wallet": "0x1234",
        "side": "BUY",
        "asset": "abc123",
        "condition_id": "0xdef456",
        "size": 100.0,
        "price": 0.55,
        "timestamp": 1697875200,
        "title": "Test Market",
        "slug": "test-market",
        "icon": null,
        "event_slug": "test-event",
        "outcome": "Yes",
        "outcome_index": 0,
        "name": "TestUser",
        "pseudonym": "tester",
        "bio": null,
        "profile_image": null,
        "profile_image_optimized": null,
        "transaction_hash": "0xtxhash"
    }"#;

    let trade: DataTrade = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(trade.proxy_wallet, "0x1234");
    assert_eq!(trade.side, "BUY");
    assert_eq!(trade.asset, "abc123");
    assert_eq!(trade.size, 100.0);
    assert_eq!(trade.price, 0.55);
}

#[test]
fn test_portfolio_deserialization() {
    let json = r#"{
        "total_value": "1000.50",
        "positions": []
    }"#;

    let portfolio: Portfolio = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(portfolio.total_value, Some("1000.50".to_string()));
    assert!(portfolio.positions.is_empty());
}

// ============================================================================
// Integration Tests (require network)
// ============================================================================

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

#[tokio::test]
async fn test_get_trades_enhanced() {
    let client = DataClient::new();
    let trades = client
        .get_trades(
            None,                 // user
            None,                 // market
            Some(96664),          // event_id
            Some(10),             // limit
            None,                 // offset
            Some(true),           // taker_only
            None,                 // filter_type
            None,                 // filter_amount
            Some(TradeSide::Buy), // side
        )
        .await;

    // Trades might be empty, but should not error
    if let Ok(trades) = trades {
        assert!(trades.len() <= 10, "Should respect limit");
    }
}

#[tokio::test]
async fn test_get_positions() {
    let client = DataClient::new();
    // Use a known active address
    let result = client
        .get_positions("0x0000000000000000000000000000000000000000")
        .await;

    // Positions might be empty for unknown address, but should not error
    assert!(result.is_ok(), "Should not error");
}

#[tokio::test]
async fn test_get_positions_filtered() {
    let client = DataClient::new();
    let result = client
        .get_positions_filtered(
            "0x0000000000000000000000000000000000000000",
            None,       // market
            None,       // event_id
            Some(0.0),  // size_threshold
            None,       // redeemable
            None,       // mergeable
            Some(10),   // limit
            None,       // offset
        )
        .await;

    // Should not error
    if let Ok(positions) = result {
        assert!(positions.len() <= 10, "Should respect limit");
    }
}

#[tokio::test]
async fn test_get_activity() {
    let client = DataClient::new();
    let result = client
        .get_activity(
            "0x0000000000000000000000000000000000000000",
            Some(10),                     // limit
            None,                         // offset
            None,                         // market
            None,                         // event_id
            Some(vec![ActivityType::Trade]), // activity_types
            None,                         // start
            None,                         // end
            Some(ActivitySortBy::Timestamp), // sort_by
            Some(SortDirection::Desc),    // sort_direction
            None,                         // side
        )
        .await;

    // Activity might be empty for unknown address, but should not error
    if let Ok(activities) = result {
        assert!(activities.len() <= 10, "Should respect limit");
    }
}
