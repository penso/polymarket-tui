//! WebSocket data update types
//!
//! This module contains types for all the market data updates received via WebSocket,
//! including orderbook updates, trades, orders, prices, and errors.

use serde::{Deserialize, Serialize};

/// Orderbook update containing current bid and ask levels
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

/// A single price level in an orderbook (bid or ask)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PriceLevel {
    pub price: String,
    pub size: String,
}

/// Trade update representing a completed trade
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

/// Order update representing changes to an order
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

/// Price update for an asset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceUpdate {
    pub market: String,
    #[serde(rename = "asset_id")]
    pub asset_id: String,
    pub price: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

/// Error message received from the WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
