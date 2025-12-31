use crate::error::{PolymarketError, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[cfg(feature = "tracing")]
use tracing::{error, warn};

const RTDS_WS_URL: &str = "wss://ws-live-data.polymarket.com/";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RTDSSubscription {
    pub action: String, // "subscribe" or "unsubscribe"
    pub subscriptions: Vec<SubscriptionTopic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionTopic {
    pub topic: String, // "activity", "comments", etc.
    #[serde(rename = "type")]
    pub topic_type: String, // "*", "orders_matched", etc.
    pub filters: String, // JSON string with filters
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RTDSMessage {
    #[serde(rename = "connection_id")]
    pub connection_id: Option<String>,
    pub payload: ActivityPayload,
    pub timestamp: i64,
    pub topic: String,
    #[serde(rename = "type")]
    pub message_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityPayload {
    pub asset: String,
    pub side: String, // "BUY" or "SELL"
    pub price: f64,
    pub size: f64,
    pub timestamp: i64,
    pub title: String,
    pub slug: String,
    #[serde(rename = "eventSlug")]
    pub event_slug: String,
    pub outcome: String, // "Yes" or "No"
    #[serde(rename = "outcomeIndex")]
    pub outcome_index: i32,
    pub name: String,
    pub pseudonym: String,
    #[serde(rename = "proxyWallet")]
    pub proxy_wallet: String,
    #[serde(rename = "transactionHash")]
    pub transaction_hash: String,
    #[serde(rename = "conditionId")]
    pub condition_id: Option<String>,
    pub bio: Option<String>,
    pub icon: Option<String>,
    pub profile_image: Option<String>,
}

pub struct RTDSClient {
    event_slug: Option<String>,
    event_id: Option<u64>,
}

impl RTDSClient {
    pub fn new() -> Self {
        Self {
            event_slug: None,
            event_id: None,
        }
    }

    pub fn with_event_slug(mut self, event_slug: String) -> Self {
        self.event_slug = Some(event_slug);
        self
    }

    pub fn with_event_id(mut self, event_id: u64) -> Self {
        self.event_id = Some(event_id);
        self
    }

    pub async fn connect_and_listen<F>(&self, mut on_update: F) -> Result<()>
    where
        F: FnMut(RTDSMessage) + Send,
    {
        let (ws_stream, _) = connect_async(RTDS_WS_URL)
            .await
            .map_err(|e| PolymarketError::WebSocket(format!("Failed to connect to RTDS WebSocket: {}", e)))?;

        let (mut write, mut read) = ws_stream.split();

        // Build subscription message
        let mut subscriptions = Vec::new();

        // Subscribe to activity (trades/orders_matched)
        if let Some(ref event_slug) = self.event_slug {
            let filters = serde_json::json!({
                "event_slug": event_slug
            });
            subscriptions.push(SubscriptionTopic {
                topic: "activity".to_string(),
                topic_type: "orders_matched".to_string(),
                filters: serde_json::to_string(&filters)
                    .map_err(|e| PolymarketError::Serialization(e))?,
            });
        }

        // Subscribe to comments if event_id is provided
        if let Some(event_id) = self.event_id {
            let filters = serde_json::json!({
                "parentEntityID": event_id,
                "parentEntityType": "Event"
            });
            subscriptions.push(SubscriptionTopic {
                topic: "comments".to_string(),
                topic_type: "*".to_string(),
                filters: serde_json::to_string(&filters)
                    .map_err(|e| PolymarketError::Serialization(e))?,
            });
        }

        if subscriptions.is_empty() {
            return Err(crate::error::PolymarketError::InvalidData(
                "No subscriptions configured. Provide event_slug or event_id.".to_string(),
            ));
        }

        let subscribe_msg = RTDSSubscription {
            action: "subscribe".to_string(),
            subscriptions,
        };

        let subscribe_json = serde_json::to_string(&subscribe_msg)
            .map_err(|e| PolymarketError::Serialization(e))?;
        write
            .send(Message::Text(subscribe_json))
            .await
            .map_err(|e| PolymarketError::WebSocket(format!("Failed to send RTDS subscription message: {}", e)))?;

        // Listen for messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Try to parse as RTDS message
                    if let Ok(rtds_msg) = serde_json::from_str::<RTDSMessage>(&text) {
                        on_update(rtds_msg);
                    } else if text == "PING" {
                        // Respond to ping
                        if let Err(e) = write.send(Message::Text("PONG".to_string())).await {
                            #[cfg(feature = "tracing")]
                            error!("Failed to send PONG: {}", e);
                            #[cfg(not(feature = "tracing"))]
                            eprintln!("Failed to send PONG: {}", e);
                            break;
                        }
                    } else {
                        // Unknown message, log for debugging
                        #[cfg(feature = "tracing")]
                        warn!("Unknown RTDS message: {}", text);
                        #[cfg(not(feature = "tracing"))]
                        eprintln!("Unknown RTDS message: {}", text);
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
                    error!("RTDS WebSocket error: {}", e);
                    #[cfg(not(feature = "tracing"))]
                    eprintln!("RTDS WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }
}

