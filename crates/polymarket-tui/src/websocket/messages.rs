//! WebSocket subscription and control messages
//!
//! This module contains types for subscribing to and managing WebSocket connections
//! to the Polymarket WebSocket API.

use serde::{Deserialize, Serialize};

/// Message sent to subscribe to market data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets_ids: Option<Vec<String>>,
    #[serde(rename = "type")]
    pub channel_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_feature_enabled: Option<bool>,
}

/// Authentication credentials for authenticated WebSocket connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Auth {
    pub api_key: String,
    pub api_secret: String,
    pub timestamp: String,
    pub signature: String,
}

/// Message to update an existing subscription (subscribe or unsubscribe)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSubscriptionMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markets: Option<Vec<String>>,
    pub operation: String, // "subscribe" or "unsubscribe"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_feature_enabled: Option<bool>,
}

/// Confirmation message received after successful subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribedMessage {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markets: Option<Vec<String>>,
}
