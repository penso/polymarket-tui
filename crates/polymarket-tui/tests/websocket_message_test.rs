//! Tests for WebSocketMessage enum

use polymarket_tui::websocket::{
    messages::SubscribedMessage,
    types::{ErrorMessage, OrderUpdate, OrderbookUpdate, PriceLevel, PriceUpdate, TradeUpdate},
    WebSocketMessage,
};

#[test]
fn test_websocket_message_orderbook() {
    let orderbook = OrderbookUpdate {
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

    let msg = WebSocketMessage::Orderbook(orderbook.clone());

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("\"type\":\"orderbook\""));
    assert!(json.contains("0x123"));
    assert!(json.contains("0.5"));
}

#[test]
fn test_websocket_message_trade() {
    let trade = TradeUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        price: "0.55".to_string(),
        size: "150".to_string(),
        side: "buy".to_string(),
        timestamp: Some(1234567890),
    };

    let msg = WebSocketMessage::Trade(trade.clone());

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("\"type\":\"trade\""));
    assert!(json.contains("0.55"));
    assert!(json.contains("buy"));
}

#[test]
fn test_websocket_message_order() {
    let order = OrderUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        side: "buy".to_string(),
        price: "0.5".to_string(),
        size: "100".to_string(),
        status: "open".to_string(),
        timestamp: Some(1234567890),
    };

    let msg = WebSocketMessage::Order(order.clone());

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("\"type\":\"order\""));
    assert!(json.contains("open"));
}

#[test]
fn test_websocket_message_price() {
    let price = PriceUpdate {
        market: "0x123".to_string(),
        asset_id: "0x456".to_string(),
        price: "0.55".to_string(),
        timestamp: Some(1234567890),
    };

    let msg = WebSocketMessage::Price(price.clone());

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("\"type\":\"price\""));
    assert!(json.contains("0.55"));
}

#[test]
fn test_websocket_message_error() {
    let error = ErrorMessage {
        error: "Invalid subscription".to_string(),
        message: Some("Details here".to_string()),
    };

    let msg = WebSocketMessage::Error(error.clone());

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("\"type\":\"error\""));
    assert!(json.contains("Invalid subscription"));
}

#[test]
fn test_websocket_message_subscribed() {
    let subscribed = SubscribedMessage {
        message: "Subscribed successfully".to_string(),
        assets_ids: Some(vec!["token1".to_string()]),
        markets: None,
    };

    let msg = WebSocketMessage::Subscribed(subscribed.clone());

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("\"type\":\"subscribed\""));
    assert!(json.contains("Subscribed successfully"));
}

#[test]
fn test_websocket_message_deserialization_orderbook() {
    let json = r#"{
        "type": "orderbook",
        "market": "0x123",
        "asset_id": "0x456",
        "bids": [{"price": "0.5", "size": "100"}],
        "asks": [{"price": "0.6", "size": "200"}],
        "timestamp": 1234567890
    }"#;

    let msg: WebSocketMessage = serde_json::from_str(json).expect("Should deserialize");
    match msg {
        WebSocketMessage::Orderbook(update) => {
            assert_eq!(update.market, "0x123");
            assert_eq!(update.asset_id, "0x456");
        }
        _ => panic!("Expected Orderbook variant"),
    }
}

#[test]
fn test_websocket_message_deserialization_trade() {
    let json = r#"{
        "type": "trade",
        "market": "0x123",
        "asset_id": "0x456",
        "price": "0.55",
        "size": "150",
        "side": "buy",
        "timestamp": 1234567890
    }"#;

    let msg: WebSocketMessage = serde_json::from_str(json).expect("Should deserialize");
    match msg {
        WebSocketMessage::Trade(update) => {
            assert_eq!(update.price, "0.55");
            assert_eq!(update.side, "buy");
        }
        _ => panic!("Expected Trade variant"),
    }
}

#[test]
fn test_websocket_message_deserialization_order() {
    let json = r#"{
        "type": "order",
        "market": "0x123",
        "asset_id": "0x456",
        "side": "buy",
        "price": "0.5",
        "size": "100",
        "status": "filled",
        "timestamp": 1234567890
    }"#;

    let msg: WebSocketMessage = serde_json::from_str(json).expect("Should deserialize");
    match msg {
        WebSocketMessage::Order(update) => {
            assert_eq!(update.status, "filled");
            assert_eq!(update.side, "buy");
        }
        _ => panic!("Expected Order variant"),
    }
}

#[test]
fn test_websocket_message_deserialization_price() {
    let json = r#"{
        "type": "price",
        "market": "0x123",
        "asset_id": "0x456",
        "price": "0.75",
        "timestamp": 1234567890
    }"#;

    let msg: WebSocketMessage = serde_json::from_str(json).expect("Should deserialize");
    match msg {
        WebSocketMessage::Price(update) => {
            assert_eq!(update.price, "0.75");
        }
        _ => panic!("Expected Price variant"),
    }
}

#[test]
fn test_websocket_message_deserialization_error() {
    let json = r#"{
        "type": "error",
        "error": "Invalid request",
        "message": "Missing field"
    }"#;

    let msg: WebSocketMessage = serde_json::from_str(json).expect("Should deserialize");
    match msg {
        WebSocketMessage::Error(err) => {
            assert_eq!(err.error, "Invalid request");
            assert_eq!(err.message, Some("Missing field".to_string()));
        }
        _ => panic!("Expected Error variant"),
    }
}

#[test]
fn test_websocket_message_deserialization_subscribed() {
    let json = r#"{
        "type": "subscribed",
        "message": "Successfully subscribed",
        "assets_ids": ["token1", "token2"]
    }"#;

    let msg: WebSocketMessage = serde_json::from_str(json).expect("Should deserialize");
    match msg {
        WebSocketMessage::Subscribed(sub) => {
            assert_eq!(sub.message, "Successfully subscribed");
            assert_eq!(
                sub.assets_ids,
                Some(vec!["token1".to_string(), "token2".to_string()])
            );
        }
        _ => panic!("Expected Subscribed variant"),
    }
}

#[test]
fn test_websocket_message_unknown_type() {
    let json = r#"{
        "type": "unknown_type",
        "data": "some data"
    }"#;

    let msg: WebSocketMessage = serde_json::from_str(json).expect("Should deserialize as Unknown");
    match msg {
        WebSocketMessage::Unknown => {}
        _ => panic!("Expected Unknown variant for unknown type"),
    }
}

#[test]
fn test_websocket_message_no_type_field() {
    // When the type field is missing, deserialization should fail
    // because serde requires the tag field for tagged enums
    let json = r#"{
        "data": "some data"
    }"#;

    let result: Result<WebSocketMessage, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "Deserialization should fail when type field is missing"
    );
}

#[test]
fn test_round_trip_websocket_message_orderbook() {
    let original = WebSocketMessage::Orderbook(OrderbookUpdate {
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
    });

    let json = serde_json::to_string(&original).expect("Should serialize");
    let deserialized: WebSocketMessage = serde_json::from_str(&json).expect("Should deserialize");

    match (original, deserialized) {
        (WebSocketMessage::Orderbook(orig), WebSocketMessage::Orderbook(deser)) => {
            assert_eq!(orig.market, deser.market);
            assert_eq!(orig.asset_id, deser.asset_id);
        }
        _ => panic!("Round trip failed"),
    }
}
