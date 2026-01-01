//! Tests for WebSocket message types (subscription, auth, etc.)

use polymarket_bot::websocket::messages::{
    Auth, SubscribedMessage, SubscriptionMessage, UpdateSubscriptionMessage,
};

#[test]
fn test_subscription_message_serialization_minimal() {
    let msg = SubscriptionMessage {
        auth: None,
        markets: None,
        assets_ids: Some(vec!["test_token_1".to_string(), "test_token_2".to_string()]),
        channel_type: "market".to_string(),
        custom_feature_enabled: None,
    };

    let json = serde_json::to_string(&msg).expect("Should serialize");

    // Verify the JSON structure
    assert!(json.contains("\"type\":\"market\""));
    assert!(json.contains("test_token_1"));
    assert!(json.contains("test_token_2"));
    assert!(!json.contains("auth")); // Should be omitted when None
    assert!(!json.contains("markets")); // Should be omitted when None
    assert!(!json.contains("custom_feature_enabled")); // Should be omitted when None
}

#[test]
fn test_subscription_message_serialization_full() {
    let auth = Auth {
        api_key: "test_key".to_string(),
        api_secret: "test_secret".to_string(),
        timestamp: "1234567890".to_string(),
        signature: "test_sig".to_string(),
    };

    let msg = SubscriptionMessage {
        auth: Some(auth),
        markets: Some(vec!["market1".to_string(), "market2".to_string()]),
        assets_ids: Some(vec!["asset1".to_string()]),
        channel_type: "market".to_string(),
        custom_feature_enabled: Some(true),
    };

    let json = serde_json::to_string(&msg).expect("Should serialize");

    assert!(json.contains("\"type\":\"market\""));
    assert!(json.contains("market1"));
    assert!(json.contains("asset1"));
    assert!(json.contains("test_key"));
    assert!(json.contains("custom_feature_enabled"));
}

#[test]
fn test_subscription_message_deserialization() {
    let json = r#"{
        "type": "market",
        "assets_ids": ["token1", "token2"],
        "markets": ["market1"],
        "custom_feature_enabled": true
    }"#;

    let msg: SubscriptionMessage = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(msg.channel_type, "market");
    assert_eq!(
        msg.assets_ids,
        Some(vec!["token1".to_string(), "token2".to_string()])
    );
    assert_eq!(msg.markets, Some(vec!["market1".to_string()]));
    assert_eq!(msg.custom_feature_enabled, Some(true));
    assert!(msg.auth.is_none());
}

#[test]
fn test_auth_serialization() {
    let auth = Auth {
        api_key: "key123".to_string(),
        api_secret: "secret456".to_string(),
        timestamp: "1234567890".to_string(),
        signature: "sig789".to_string(),
    };

    let json = serde_json::to_string(&auth).expect("Should serialize");
    assert!(json.contains("key123"));
    assert!(json.contains("secret456"));
    assert!(json.contains("1234567890"));
    assert!(json.contains("sig789"));
}

#[test]
fn test_auth_deserialization() {
    let json = r#"{
        "api_key": "test_key",
        "api_secret": "test_secret",
        "timestamp": "1234567890",
        "signature": "test_sig"
    }"#;

    let auth: Auth = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(auth.api_key, "test_key");
    assert_eq!(auth.api_secret, "test_secret");
    assert_eq!(auth.timestamp, "1234567890");
    assert_eq!(auth.signature, "test_sig");
}

#[test]
fn test_update_subscription_message_subscribe() {
    let msg = UpdateSubscriptionMessage {
        assets_ids: Some(vec!["asset1".to_string()]),
        markets: None,
        operation: "subscribe".to_string(),
        custom_feature_enabled: None,
    };

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("\"operation\":\"subscribe\""));
    assert!(json.contains("asset1"));
    assert!(!json.contains("markets"));
}

#[test]
fn test_update_subscription_message_unsubscribe() {
    let msg = UpdateSubscriptionMessage {
        assets_ids: None,
        markets: Some(vec!["market1".to_string()]),
        operation: "unsubscribe".to_string(),
        custom_feature_enabled: Some(false),
    };

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("\"operation\":\"unsubscribe\""));
    assert!(json.contains("market1"));
    assert!(!json.contains("assets_ids"));
}

#[test]
fn test_update_subscription_message_deserialization() {
    let json = r#"{
        "operation": "subscribe",
        "assets_ids": ["token1"],
        "custom_feature_enabled": true
    }"#;

    let msg: UpdateSubscriptionMessage = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(msg.operation, "subscribe");
    assert_eq!(msg.assets_ids, Some(vec!["token1".to_string()]));
    assert_eq!(msg.custom_feature_enabled, Some(true));
    assert!(msg.markets.is_none());
}

#[test]
fn test_subscribed_message_with_assets() {
    let msg = SubscribedMessage {
        message: "Successfully subscribed".to_string(),
        assets_ids: Some(vec!["asset1".to_string(), "asset2".to_string()]),
        markets: None,
    };

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("Successfully subscribed"));
    assert!(json.contains("asset1"));
    assert!(json.contains("asset2"));
    assert!(!json.contains("markets"));
}

#[test]
fn test_subscribed_message_with_markets() {
    let msg = SubscribedMessage {
        message: "Subscribed to markets".to_string(),
        assets_ids: None,
        markets: Some(vec!["market1".to_string()]),
    };

    let json = serde_json::to_string(&msg).expect("Should serialize");
    assert!(json.contains("Subscribed to markets"));
    assert!(json.contains("market1"));
    assert!(!json.contains("assets_ids"));
}

#[test]
fn test_subscribed_message_deserialization() {
    let json = r#"{
        "message": "Subscribed successfully",
        "assets_ids": ["token1", "token2"],
        "markets": ["market1"]
    }"#;

    let msg: SubscribedMessage = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(msg.message, "Subscribed successfully");
    assert_eq!(
        msg.assets_ids,
        Some(vec!["token1".to_string(), "token2".to_string()])
    );
    assert_eq!(msg.markets, Some(vec!["market1".to_string()]));
}

#[test]
fn test_subscribed_message_minimal() {
    let json = r#"{
        "message": "Subscribed"
    }"#;

    let msg: SubscribedMessage = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(msg.message, "Subscribed");
    assert!(msg.assets_ids.is_none());
    assert!(msg.markets.is_none());
}

#[test]
fn test_round_trip_subscription_message() {
    let original = SubscriptionMessage {
        auth: None,
        markets: Some(vec!["m1".to_string()]),
        assets_ids: Some(vec!["a1".to_string()]),
        channel_type: "market".to_string(),
        custom_feature_enabled: Some(false),
    };

    let json = serde_json::to_string(&original).expect("Should serialize");
    let deserialized: SubscriptionMessage =
        serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(original.channel_type, deserialized.channel_type);
    assert_eq!(original.markets, deserialized.markets);
    assert_eq!(original.assets_ids, deserialized.assets_ids);
    assert_eq!(
        original.custom_feature_enabled,
        deserialized.custom_feature_enabled
    );
}
