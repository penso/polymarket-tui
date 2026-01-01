use crate::error::{PolymarketError, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[cfg(feature = "tracing")]
use tracing::{error, info, warn};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clob_auth: Option<ClobAuth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gamma_auth: Option<GammaAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClobAuth {
    pub key: String,
    pub secret: String,
    pub passphrase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GammaAuth {
    pub address: String,
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
    clob_auth: Option<ClobAuth>,
    gamma_auth: Option<GammaAuth>,
}

impl RTDSClient {
    pub fn new() -> Self {
        // Try to load authentication from environment variables
        // Note: Activity subscriptions (orders_matched) typically don't require auth
        // Only include auth if explicitly needed for protected subscriptions
        let clob_auth = match (
            std::env::var("api_key"),
            std::env::var("secret"),
            std::env::var("passphrase"),
        ) {
            (Ok(key), Ok(secret), Ok(passphrase)) => {
                // Validate secret length (must be 44 characters as per API)
                if secret.len() != 44 {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(
                        "CLOB secret length is {} (expected 44). Authentication may fail.",
                        secret.len()
                    );
                    #[cfg(not(feature = "tracing"))]
                    eprintln!(
                        "Warning: CLOB secret length is {} (expected 44). Authentication may fail.",
                        secret.len()
                    );
                }
                Some(ClobAuth {
                    key,
                    secret,
                    passphrase,
                })
            }
            _ => None,
        };

        let gamma_auth = std::env::var("gamma_address")
            .ok()
            .map(|address| GammaAuth { address });

        Self {
            event_slug: None,
            event_id: None,
            clob_auth,
            gamma_auth,
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

    pub fn with_clob_auth(mut self, key: String, secret: String, passphrase: String) -> Self {
        self.clob_auth = Some(ClobAuth {
            key,
            secret,
            passphrase,
        });
        self
    }

    pub fn with_gamma_auth(mut self, address: String) -> Self {
        self.gamma_auth = Some(GammaAuth { address });
        self
    }

    pub async fn connect_and_listen<F>(&self, mut on_update: F) -> Result<()>
    where
        F: FnMut(RTDSMessage) + Send,
    {
        #[cfg(feature = "tracing")]
        info!("Connecting to RTDS WebSocket: {}", RTDS_WS_URL);

        let (ws_stream, _) = connect_async(RTDS_WS_URL)
            .await
            .map_err(|e| PolymarketError::WebSocket(format!("Failed to connect to RTDS WebSocket: {}", e)))?;

        #[cfg(feature = "tracing")]
        info!("Connected to RTDS WebSocket");

        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));

        // Build subscription message
        let mut subscriptions = Vec::new();

        // Subscribe to activity (trades/orders_matched)
        // Note: Activity subscriptions are public and typically don't require CLOB auth
        // Only include auth if explicitly needed (some endpoints may require it)
        if let Some(ref event_slug) = self.event_slug {
            let filters = serde_json::json!({
                "event_slug": event_slug
            });
            subscriptions.push(SubscriptionTopic {
                topic: "activity".to_string(),
                topic_type: "orders_matched".to_string(),
                filters: serde_json::to_string(&filters)
                    .map_err(PolymarketError::Serialization)?,
                clob_auth: None, // Activity subscriptions don't require CLOB auth (public data)
                gamma_auth: self.gamma_auth.clone(),
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
                    .map_err(PolymarketError::Serialization)?,
                clob_auth: None, // Comments don't need CLOB auth
                gamma_auth: self.gamma_auth.clone(), // Comments might need gamma auth
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
            .map_err(PolymarketError::Serialization)?;

        #[cfg(feature = "tracing")]
        info!("Sending RTDS subscription: {}", subscribe_json);

        {
            let mut w = write.lock().await;
            w.send(Message::Text(subscribe_json.clone()))
                .await
                .map_err(|e| PolymarketError::WebSocket(format!("Failed to send RTDS subscription message: {}", e)))?;
        }

        #[cfg(feature = "tracing")]
        info!("RTDS subscription sent successfully");

        // Start PING task (send PING every 5 seconds as per RTDS docs)
        let write_ping = Arc::clone(&write);
        let ping_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                let mut w = write_ping.lock().await;
                if let Err(e) = w.send(Message::Text("PING".to_string())).await {
                    #[cfg(feature = "tracing")]
                    error!("Failed to send PING: {}", e);
                    #[cfg(not(feature = "tracing"))]
                    eprintln!("Failed to send PING: {}", e);
                    break;
                }
            }
        });

        // Listen for messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Skip empty messages
                    if text.trim().is_empty() {
                        continue;
                    }

                    // Try to parse as RTDS message
                    if let Ok(rtds_msg) = serde_json::from_str::<RTDSMessage>(&text) {
                        #[cfg(feature = "tracing")]
                        info!("Received RTDS message: topic={}, type={}", rtds_msg.topic, rtds_msg.message_type);
                        on_update(rtds_msg);
                        continue; // Successfully handled, move to next message
                    }

                    // Handle PING/PONG
                    if text.as_str() == "PING" {
                        // Respond to ping (though we're the ones sending PING, server might send it too)
                        let mut w = write.lock().await;
                        if let Err(e) = w.send(Message::Text("PONG".to_string())).await {
                            #[cfg(feature = "tracing")]
                            error!("Failed to send PONG: {}", e);
                            #[cfg(not(feature = "tracing"))]
                            eprintln!("Failed to send PONG: {}", e);
                            break;
                        }
                    } else if text == "PONG" {
                        // Server responded to our PING, just continue silently
                        continue;
                    } else {
                        // Try to parse as error message
                        if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(body) = error_json.get("body") {
                                if let Some(message) = body.get("message").and_then(|m| m.as_str()) {
                                    #[cfg(feature = "tracing")]
                                    {
                                        error!("RTDS error: {}", message);
                                    }
                                    #[cfg(not(feature = "tracing"))]
                                    {
                                        eprintln!("RTDS error: {}", message);
                                    }
                                    // If it's an authentication error, break the connection
                                    if message.contains("validation") || message.contains("auth") {
                                        break;
                                    }
                                    continue;
                                }
                            }
                        }

                        // If we get here, it's a truly unknown message format
                        #[cfg(feature = "tracing")]
                        {
                            warn!("Unknown RTDS message format: {}", if text.len() > 200 { &text[..200] } else { &text });
                        }
                        #[cfg(not(feature = "tracing"))]
                        {
                            eprintln!("Unknown RTDS message format: {}", if text.len() > 200 { &text[..200] } else { &text });
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    // Respond to ping with pong
                    let mut w = write.lock().await;
                    if let Err(e) = w.send(Message::Pong(data)).await {
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

        // Cancel PING task
        ping_handle.abort();

        Ok(())
    }
}

