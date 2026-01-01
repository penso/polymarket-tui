//! CLOB (Central Limit Order Book) REST API client
//!
//! This module provides a client for interacting with Polymarket's CLOB REST API,
//! which allows fetching orderbooks, trades, and managing orders.

use crate::error::Result;
use serde::{Deserialize, Serialize};

const CLOB_API_BASE: &str = "https://clob.polymarket.com";

/// Order side (buy or sell)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    Buy,
    Sell,
}

/// Order type
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderType {
    Limit,
    Market,
}

/// Order status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
    Rejected,
}

/// Price level in the orderbook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: String,
    pub size: String,
}

/// Orderbook snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Orderbook {
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
}

/// Trade information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub price: String,
    pub size: String,
    pub timestamp: i64,
    pub side: String,
    pub maker_order_id: Option<String>,
    pub taker_order_id: Option<String>,
}

/// Order information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: String,
    pub market: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub price: Option<String>,
    pub size: String,
    pub filled: String,
    pub status: String,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}

/// CLOB REST API client
pub struct ClobClient {
    client: reqwest::Client,
    #[allow(dead_code)] // Will be used for authentication signing
    api_key: Option<String>,
    #[allow(dead_code)] // Will be used for authentication signing
    api_secret: Option<String>,
    #[allow(dead_code)] // Will be used for authentication signing
    passphrase: Option<String>,
}

impl ClobClient {
    /// Create a new CLOB client without authentication (for public endpoints)
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: None,
            api_secret: None,
            passphrase: None,
        }
    }

    /// Create a new CLOB client with authentication
    pub fn with_auth(api_key: String, api_secret: String, passphrase: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: Some(api_key),
            api_secret: Some(api_secret),
            passphrase: Some(passphrase),
        }
    }

    /// Create a new CLOB client from environment variables
    pub fn from_env() -> Self {
        if let (Ok(api_key), Ok(api_secret), Ok(passphrase)) = (
            std::env::var("api_key"),
            std::env::var("secret"),
            std::env::var("passphrase"),
        ) {
            Self::with_auth(api_key, api_secret, passphrase)
        } else {
            Self::new()
        }
    }

    /// Get orderbook for a specific market (condition ID)
    pub async fn get_orderbook(&self, condition_id: &str) -> Result<Orderbook> {
        let url = format!("{}/book", CLOB_API_BASE);
        let params = [("market", condition_id)];
        let orderbook: Orderbook = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(orderbook)
    }

    /// Get recent trades for a specific market (condition ID)
    pub async fn get_trades(
        &self,
        condition_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>> {
        let url = format!("{}/trades", CLOB_API_BASE);
        let mut params = vec![("market", condition_id.to_string())];
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        let trades: Vec<Trade> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(trades)
    }

    /// Get orderbook for a specific asset ID
    pub async fn get_orderbook_by_asset(&self, asset_id: &str) -> Result<Orderbook> {
        let url = format!("{}/book", CLOB_API_BASE);
        let params = [("asset_id", asset_id)];
        let orderbook: Orderbook = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(orderbook)
    }

    /// Get recent trades for a specific asset ID
    pub async fn get_trades_by_asset(
        &self,
        asset_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>> {
        let url = format!("{}/trades", CLOB_API_BASE);
        let mut params = vec![("asset_id", asset_id.to_string())];
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        let trades: Vec<Trade> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(trades)
    }

    /// Get user's orders (requires authentication)
    pub async fn get_orders(&self) -> Result<Vec<Order>> {
        let url = format!("{}/orders", CLOB_API_BASE);
        // TODO: Add authentication headers
        let orders: Vec<Order> = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;
        Ok(orders)
    }

    /// Get a specific order by ID (requires authentication)
    pub async fn get_order(&self, order_id: &str) -> Result<Order> {
        let url = format!("{}/orders/{}", CLOB_API_BASE, order_id);
        // TODO: Add authentication headers
        let order: Order = self.client.get(&url).send().await?.json().await?;
        Ok(order)
    }

    /// Place a new order (requires authentication)
    pub async fn place_order(
        &self,
        _market: &str,
        _side: Side,
        _order_type: OrderType,
        _size: &str,
        _price: Option<&str>,
    ) -> Result<Order> {
        // TODO: Add authentication headers and request body
        // This is a placeholder - actual implementation would need proper auth signing
        let _url = format!("{}/orders", CLOB_API_BASE);
        todo!("Order placement requires authentication signing")
    }

    /// Cancel an order (requires authentication)
    pub async fn cancel_order(&self, order_id: &str) -> Result<()> {
        let url = format!("{}/orders/{}", CLOB_API_BASE, order_id);
        // TODO: Add authentication headers
        self.client.delete(&url).send().await?;
        Ok(())
    }
}

impl Default for ClobClient {
    fn default() -> Self {
        Self::new()
    }
}

