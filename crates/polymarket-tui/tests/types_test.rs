//! Tests for WebSocket data update types

use polymarket_tui::websocket::types::{
    ErrorMessage, OrderUpdate, OrderbookUpdate, PriceLevel, PriceUpdate, TradeUpdate,
};

#[test]
fn test_price_level_serialization() {
    let level = PriceLevel {
        price: "0.5".to_string(),
        size: "100".to_string(),
    };

    let json = serde_json::to_string(&level).expect("Should serialize");
    assert!(json.contains("\"price\":\"0.5\""));
    assert!(json.contains("\"size\":\"100\""));
}

#[test]
fn test_price_level_deserialization() {
    let json = r#"{"price": "0.75", "size": "250"}"#;
    let level: PriceLevel = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(level.price, "0.75");
    assert_eq!(level.size, "250");
}

#[test]
fn test_price_level_equality() {
    let level1 = PriceLevel {
        price: "0.5".to_string(),
        size: "100".to_string(),
    };
    let level2 = PriceLevel {
        price: "0.5".to_string(),
        size: "100".to_string(),
    };
    let level3 = PriceLevel {
        price: "0.6".to_string(),
        size: "100".to_string(),
    };

    assert_eq!(level1, level2);
    assert_ne!(level1, level3);
}

#[test]
fn test_orderbook_update_with_timestamp() {
    let update = OrderbookUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        bids: vec![
            PriceLevel {
                price: "0.4".to_string(),
                size: "50".to_string(),
            },
            PriceLevel {
                price: "0.3".to_string(),
                size: "100".to_string(),
            },
        ],
        asks: vec![PriceLevel {
            price: "0.6".to_string(),
            size: "75".to_string(),
        }],
        timestamp: Some(1234567890),
    };

    let json = serde_json::to_string(&update).expect("Should serialize");
    assert!(json.contains("0x123"));
    assert!(json.contains("\"asset_id\":\"0x456\""));
    assert!(json.contains("0.4"));
    assert!(json.contains("0.6"));
    assert!(json.contains("1234567890"));
}

#[test]
fn test_orderbook_update_without_timestamp() {
    let update = OrderbookUpdate {
        market: "0x789".to_string(),
        asset_id: "0xabc".to_string(),
        bids: vec![],
        asks: vec![],
        timestamp: None,
    };

    let json = serde_json::to_string(&update).expect("Should serialize");
    assert!(!json.contains("timestamp")); // Should be omitted when None
}

#[test]
fn test_orderbook_update_deserialization() {
    let json = r#"{
        "market": "0x123",
        "asset_id": "0x456",
        "bids": [{"price": "0.5", "size": "100"}],
        "asks": [{"price": "0.6", "size": "200"}],
        "timestamp": 1234567890
    }"#;

    let update: OrderbookUpdate = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(update.market, "0x123");
    assert_eq!(update.asset_id, "0x456");
    assert_eq!(update.bids.len(), 1);
    assert_eq!(update.asks.len(), 1);
    assert_eq!(update.bids[0].price, "0.5");
    assert_eq!(update.asks[0].price, "0.6");
    assert_eq!(update.timestamp, Some(1234567890));
}

#[test]
fn test_trade_update_buy() {
    let update = TradeUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        price: "0.55".to_string(),
        size: "150".to_string(),
        side: "buy".to_string(),
        timestamp: Some(1234567890),
    };

    let json = serde_json::to_string(&update).expect("Should serialize");
    assert!(json.contains("\"side\":\"buy\""));
    assert!(json.contains("0.55"));
    assert!(json.contains("150"));
}

#[test]
fn test_trade_update_sell() {
    let update = TradeUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        price: "0.45".to_string(),
        size: "200".to_string(),
        side: "sell".to_string(),
        timestamp: None,
    };

    let json = serde_json::to_string(&update).expect("Should serialize");
    assert!(json.contains("\"side\":\"sell\""));
    assert!(!json.contains("timestamp"));
}

#[test]
fn test_trade_update_deserialization() {
    let json = r#"{
        "market": "0x123",
        "asset_id": "0x456",
        "price": "0.55",
        "size": "150",
        "side": "buy",
        "timestamp": 1234567890
    }"#;

    let update: TradeUpdate = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(update.market, "0x123");
    assert_eq!(update.asset_id, "0x456");
    assert_eq!(update.price, "0.55");
    assert_eq!(update.size, "150");
    assert_eq!(update.side, "buy");
    assert_eq!(update.timestamp, Some(1234567890));
}

#[test]
fn test_order_update_open() {
    let update = OrderUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        side: "buy".to_string(),
        price: "0.5".to_string(),
        size: "100".to_string(),
        status: "open".to_string(),
        timestamp: Some(1234567890),
    };

    let json = serde_json::to_string(&update).expect("Should serialize");
    assert!(json.contains("\"status\":\"open\""));
    assert!(json.contains("\"side\":\"buy\""));
}

#[test]
fn test_order_update_filled() {
    let update = OrderUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        side: "sell".to_string(),
        price: "0.6".to_string(),
        size: "200".to_string(),
        status: "filled".to_string(),
        timestamp: None,
    };

    let json = serde_json::to_string(&update).expect("Should serialize");
    assert!(json.contains("\"status\":\"filled\""));
    assert!(json.contains("\"side\":\"sell\""));
}

#[test]
fn test_order_update_deserialization() {
    let json = r#"{
        "market": "0x123",
        "asset_id": "0x456",
        "side": "buy",
        "price": "0.5",
        "size": "100",
        "status": "cancelled",
        "timestamp": 1234567890
    }"#;

    let update: OrderUpdate = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(update.market, "0x123");
    assert_eq!(update.asset_id, "0x456");
    assert_eq!(update.side, "buy");
    assert_eq!(update.price, "0.5");
    assert_eq!(update.size, "100");
    assert_eq!(update.status, "cancelled");
    assert_eq!(update.timestamp, Some(1234567890));
}

#[test]
fn test_price_update_with_timestamp() {
    let update = PriceUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        price: "0.55".to_string(),
        timestamp: Some(1234567890),
    };

    let json = serde_json::to_string(&update).expect("Should serialize");
    assert!(json.contains("0.55"));
    assert!(json.contains("1234567890"));
}

#[test]
fn test_price_update_without_timestamp() {
    let update = PriceUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        price: "0.55".to_string(),
        timestamp: None,
    };

    let json = serde_json::to_string(&update).expect("Should serialize");
    assert!(!json.contains("timestamp"));
}

#[test]
fn test_price_update_deserialization() {
    let json = r#"{
        "market": "0x123",
        "asset_id": "0x456",
        "price": "0.75",
        "timestamp": 1234567890
    }"#;

    let update: PriceUpdate = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(update.market, "0x123");
    assert_eq!(update.asset_id, "0x456");
    assert_eq!(update.price, "0.75");
    assert_eq!(update.timestamp, Some(1234567890));
}

#[test]
fn test_error_message_with_message() {
    let error = ErrorMessage {
        error: "Invalid subscription".to_string(),
        message: Some("The provided asset IDs are invalid".to_string()),
    };

    let json = serde_json::to_string(&error).expect("Should serialize");
    assert!(json.contains("Invalid subscription"));
    assert!(json.contains("The provided asset IDs are invalid"));
}

#[test]
fn test_error_message_without_message() {
    let error = ErrorMessage {
        error: "Connection error".to_string(),
        message: None,
    };

    let json = serde_json::to_string(&error).expect("Should serialize");
    assert!(json.contains("Connection error"));
    assert!(!json.contains("message")); // Should be omitted when None
}

#[test]
fn test_error_message_deserialization() {
    let json = r#"{
        "error": "Invalid request",
        "message": "Missing required field"
    }"#;

    let error: ErrorMessage = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(error.error, "Invalid request");
    assert_eq!(error.message, Some("Missing required field".to_string()));
}

#[test]
fn test_error_message_minimal() {
    let json = r#"{"error": "Generic error"}"#;
    let error: ErrorMessage = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(error.error, "Generic error");
    assert!(error.message.is_none());
}

#[test]
fn test_round_trip_orderbook_update() {
    let original = OrderbookUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        bids: vec![PriceLevel {
            price: "0.5".to_string(),
            size: "100".to_string(),
        }],
        asks: vec![PriceLevel {
            price: "0.6".to_string(),
            size: "200".to_string(),
        }],
        timestamp: Some(1234567890),
    };

    let json = serde_json::to_string(&original).expect("Should serialize");
    let deserialized: OrderbookUpdate = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(original.market, deserialized.market);
    assert_eq!(original.asset_id, deserialized.asset_id);
    assert_eq!(original.bids.len(), deserialized.bids.len());
    assert_eq!(original.asks.len(), deserialized.asks.len());
    assert_eq!(original.bids[0], deserialized.bids[0]);
    assert_eq!(original.asks[0], deserialized.asks[0]);
    assert_eq!(original.timestamp, deserialized.timestamp);
}

#[test]
fn test_round_trip_trade_update() {
    let original = TradeUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        price: "0.55".to_string(),
        size: "150".to_string(),
        side: "buy".to_string(),
        timestamp: Some(1234567890),
    };

    let json = serde_json::to_string(&original).expect("Should serialize");
    let deserialized: TradeUpdate = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(original.market, deserialized.market);
    assert_eq!(original.asset_id, deserialized.asset_id);
    assert_eq!(original.price, deserialized.price);
    assert_eq!(original.size, deserialized.size);
    assert_eq!(original.side, deserialized.side);
    assert_eq!(original.timestamp, deserialized.timestamp);
}
