use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Auth {
    pub api_key: String,
    pub api_secret: String,
    pub timestamp: String,
    pub signature: String,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookUpdate {
    pub market: String,
    #[serde(rename = "asset_id")]
    pub asset_id: String,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: String,
    pub size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeUpdate {
    pub market: String,
    #[serde(rename = "asset_id")]
    pub asset_id: String,
    pub price: String,
    pub size: String,
    pub side: String, // "buy" or "sell"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderUpdate {
    pub market: String,
    #[serde(rename = "asset_id")]
    pub asset_id: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceUpdate {
    pub market: String,
    #[serde(rename = "asset_id")]
    pub asset_id: String,
    pub price: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribedMessage {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markets: Option<Vec<String>>,
}

pub struct PolymarketWebSocket {
    asset_ids: Vec<String>,
    market_info_cache: HashMap<String, crate::gamma::MarketInfo>,
}

impl PolymarketWebSocket {
    pub fn new(asset_ids: Vec<String>) -> Self {
        Self {
            asset_ids,
            market_info_cache: HashMap::new(),
        }
    }

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
            channel_type: "MARKET".to_string(),
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
                    } else if let Ok(subscribed) = serde_json::from_str::<SubscribedMessage>(&text)
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
                        eprintln!("Failed to send pong: {}", e);
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    break;
                }
                Err(e) => {
                    eprintln!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn update_market_info(&mut self, asset_id: String, info: crate::gamma::MarketInfo) {
        self.market_info_cache.insert(asset_id, info);
    }

    pub fn get_market_info(&self, asset_id: &str) -> Option<&crate::gamma::MarketInfo> {
        self.market_info_cache.get(asset_id)
    }
}
