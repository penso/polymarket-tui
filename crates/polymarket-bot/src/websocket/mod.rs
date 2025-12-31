//! Polymarket WebSocket client and types
//!
//! This module provides a WebSocket client for connecting to Polymarket's
//! real-time market data stream, along with all the data types for messages
//! and updates received over the WebSocket connection.

pub mod messages;
pub mod types;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[cfg(feature = "tracing")]
use tracing::{error, warn};

pub use messages::{
    Auth, SubscribedMessage, SubscriptionMessage, UpdateSubscriptionMessage,
};
pub use types::{
    ErrorMessage, OrderUpdate, OrderbookUpdate, PriceLevel, PriceUpdate, TradeUpdate,
};

const WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

/// Main WebSocket message enum that can represent any message type received from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WebSocketMessage {
    #[serde(rename = "orderbook")]
    Orderbook(OrderbookUpdate),
    #[serde(rename = "trade")]
    Trade(TradeUpdate),
    #[serde(rename = "order")]
    Order(OrderUpdate),
    #[serde(rename = "price")]
    Price(PriceUpdate),
    #[serde(rename = "error")]
    Error(ErrorMessage),
    #[serde(rename = "subscribed")]
    Subscribed(SubscribedMessage),
    #[serde(other)]
    Unknown,
}

/// WebSocket client for connecting to Polymarket's market data stream
pub struct PolymarketWebSocket {
    pub(crate) asset_ids: Vec<String>,
    market_info_cache: HashMap<String, crate::gamma::MarketInfo>,
}

impl PolymarketWebSocket {
    /// Create a new WebSocket client for the given asset IDs
    pub fn new(asset_ids: Vec<String>) -> Self {
        Self {
            asset_ids,
            market_info_cache: HashMap::new(),
        }
    }

    /// Connect to the WebSocket and listen for updates
    ///
    /// The callback function will be called for each message received.
    pub async fn connect_and_listen<F>(&mut self, mut on_update: F) -> Result<()>
    where
        F: FnMut(WebSocketMessage) + Send,
    {
        let (ws_stream, _) = connect_async(WS_URL)
            .await
            .context("Failed to connect to WebSocket")?;

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to market channel
        let subscribe_msg = SubscriptionMessage {
            auth: None, // No auth needed for public market data
            markets: None,
            assets_ids: Some(self.asset_ids.clone()),
            channel_type: "market".to_string(), // Use lowercase as per Polymarket docs
            custom_feature_enabled: None,
        };

        let subscribe_json = serde_json::to_string(&subscribe_msg)?;
        write
            .send(Message::Text(subscribe_json))
            .await
            .context("Failed to send subscription message")?;

        // Listen for messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Try to parse as WebSocketMessage first
                    if let Ok(ws_msg) = serde_json::from_str::<WebSocketMessage>(&text) {
                        on_update(ws_msg);
                    } else if let Ok(subscribed) =
                        serde_json::from_str::<SubscribedMessage>(&text)
                    {
                        on_update(WebSocketMessage::Subscribed(subscribed));
                    } else if let Ok(err) = serde_json::from_str::<ErrorMessage>(&text) {
                        on_update(WebSocketMessage::Error(err));
                    } else {
                        // Try to parse by checking for type field
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
                                match msg_type {
                                    "orderbook" => {
                                        if let Ok(update) =
                                            serde_json::from_value::<OrderbookUpdate>(json)
                                        {
                                            on_update(WebSocketMessage::Orderbook(update));
                                        }
                                    }
                                    "trade" => {
                                        if let Ok(update) =
                                            serde_json::from_value::<TradeUpdate>(json)
                                        {
                                            on_update(WebSocketMessage::Trade(update));
                                        }
                                    }
                                    "order" => {
                                        if let Ok(update) =
                                            serde_json::from_value::<OrderUpdate>(json)
                                        {
                                            on_update(WebSocketMessage::Order(update));
                                        }
                                    }
                                    "price" => {
                                        if let Ok(update) =
                                            serde_json::from_value::<PriceUpdate>(json)
                                        {
                                            on_update(WebSocketMessage::Price(update));
                                        }
                                    }
                                    _ => {
                                        // Unknown message type, log for debugging
                                        #[cfg(feature = "tracing")]
                                        warn!("Unknown message type: {}", text);
                                        #[cfg(not(feature = "tracing"))]
                                        eprintln!("Unknown message type: {}", text);
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    // Respond to ping with pong
                    if let Err(e) = write.send(Message::Pong(data)).await {
                        #[cfg(feature = "tracing")]
                        error!("Failed to send pong: {}", e);
                        #[cfg(not(feature = "tracing"))]
                        eprintln!("Failed to send pong: {}", e);
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    break;
                }
                Err(e) => {
                    #[cfg(feature = "tracing")]
                    error!("WebSocket error: {}", e);
                    #[cfg(not(feature = "tracing"))]
                    eprintln!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Update cached market info for an asset
    pub fn update_market_info(&mut self, asset_id: String, info: crate::gamma::MarketInfo) {
        self.market_info_cache.insert(asset_id, info);
    }

    /// Get cached market info for an asset
    pub fn get_market_info(&self, asset_id: &str) -> Option<&crate::gamma::MarketInfo> {
        self.market_info_cache.get(asset_id)
    }
}

