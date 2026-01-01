//! CLOB (Central Limit Order Book) REST API client
//!
//! This module provides a client for interacting with Polymarket's CLOB REST API,
//! which allows fetching orderbooks, trades, and managing orders.

use {
    crate::error::Result,
    serde::{Deserialize, Serialize},
};

/// Macro for conditional info logging based on tracing feature
#[cfg(feature = "tracing")]
macro_rules! log_info {
    ($($arg:tt)*) => { tracing::info!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_info {
    ($($arg:tt)*) => {};
}

/// Macro for conditional debug logging based on tracing feature
#[cfg(feature = "tracing")]
macro_rules! log_debug {
    ($($arg:tt)*) => { tracing::debug!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_debug {
    ($($arg:tt)*) => {};
}

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
    /// Market identifier (condition ID)
    #[serde(default)]
    pub market: Option<String>,
    /// Asset identifier (token ID)
    #[serde(default)]
    pub asset_id: Option<String>,
    /// Snapshot timestamp (ISO 8601)
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Order book state hash
    #[serde(default)]
    pub hash: Option<String>,
    /// Minimum tradeable size
    #[serde(default)]
    pub min_order_size: Option<String>,
    /// Minimum price increment
    #[serde(default)]
    pub tick_size: Option<String>,
    /// Whether negative risk mechanics are enabled
    #[serde(default)]
    pub neg_risk: Option<bool>,
}

/// Price response from GET /price endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceResponse {
    pub price: String,
}

/// Midpoint response from GET /midpoint endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidpointResponse {
    pub mid: String,
}

/// Historical price point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistoryPoint {
    /// Unix timestamp
    pub t: i64,
    /// Price value
    pub p: f64,
}

/// Price history response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistoryResponse {
    pub history: Vec<PriceHistoryPoint>,
}

/// Time interval for price history queries
#[derive(Debug, Clone, Copy)]
pub enum PriceInterval {
    OneMinute,
    OneHour,
    SixHours,
    OneDay,
    OneWeek,
    Max,
}

impl PriceInterval {
    /// Get the string representation of the interval
    pub fn as_str(&self) -> &'static str {
        match self {
            PriceInterval::OneMinute => "1m",
            PriceInterval::OneHour => "1h",
            PriceInterval::SixHours => "6h",
            PriceInterval::OneDay => "1d",
            PriceInterval::OneWeek => "1w",
            PriceInterval::Max => "max",
        }
    }
}

/// Request for spread calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpreadRequest {
    pub token_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
}

/// Request for batch price/orderbook queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTokenRequest {
    pub token_id: String,
    pub side: Side,
}

/// Price data for a single token (both sides)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrices {
    #[serde(rename = "BUY", default)]
    pub buy: Option<String>,
    #[serde(rename = "SELL", default)]
    pub sell: Option<String>,
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
    pub async fn get_trades(&self, condition_id: &str, limit: Option<usize>) -> Result<Vec<Trade>> {
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

    /// Get orderbook for a specific token ID (clob_token_id from Gamma API)
    pub async fn get_orderbook_by_asset(&self, token_id: &str) -> Result<Orderbook> {
        let _url = format!("{}/book?token_id={}", CLOB_API_BASE, token_id);
        log_info!("GET {}", _url);

        let params = [("token_id", token_id)];
        let response = self
            .client
            .get(format!("{}/book", CLOB_API_BASE))
            .query(&params)
            .send()
            .await?;

        let status = response.status();
        log_info!("GET {} -> status: {}", _url, status);

        // 404 means no orderbook exists for this token (market might be new or have no orders)
        if status == reqwest::StatusCode::NOT_FOUND {
            log_info!(
                "GET {} -> no orderbook (market may have no orders yet)",
                _url
            );
            return Ok(Orderbook {
                bids: Vec::new(),
                asks: Vec::new(),
                market: None,
                asset_id: Some(token_id.to_string()),
                timestamp: None,
                hash: None,
                min_order_size: None,
                tick_size: None,
                neg_risk: None,
            });
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::error::PolymarketError::InvalidData(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let response_text = response.text().await?;
        log_info!(
            "GET {} -> bids/asks preview: {}",
            _url,
            if response_text.len() > 200 {
                &response_text[..200]
            } else {
                &response_text
            }
        );

        // Try to deserialize as Orderbook
        match serde_json::from_str::<Orderbook>(&response_text) {
            Ok(orderbook) => {
                log_info!(
                    "GET {} -> parsed: {} bids, {} asks",
                    _url,
                    orderbook.bids.len(),
                    orderbook.asks.len()
                );
                Ok(orderbook)
            },
            Err(_e) => {
                // Log the actual response for debugging
                log_debug!(
                    "Failed to parse orderbook response for token {}: {}. Response: {}",
                    token_id,
                    _e,
                    response_text
                );
                // Return an empty orderbook if deserialization fails (token might not have orders)
                Ok(Orderbook {
                    bids: Vec::new(),
                    asks: Vec::new(),
                    market: None,
                    asset_id: Some(token_id.to_string()),
                    timestamp: None,
                    hash: None,
                    min_order_size: None,
                    tick_size: None,
                    neg_risk: None,
                })
            },
        }
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
        let orders: Vec<Order> = self.client.get(&url).send().await?.json().await?;
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

    /// Get market price for a specific token and side
    ///
    /// # Arguments
    /// * `token_id` - The unique identifier for the token
    /// * `side` - The side of the market (BUY or SELL)
    pub async fn get_price(&self, token_id: &str, side: Side) -> Result<PriceResponse> {
        let url = format!("{}/price", CLOB_API_BASE);
        let side_str = match side {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        };
        let params = [("token_id", token_id), ("side", side_str)];

        let response = self.client.get(&url).query(&params).send().await?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::error::PolymarketError::InvalidData(error_text));
        }

        let price: PriceResponse = response.json().await?;
        Ok(price)
    }

    /// Get midpoint price for a specific token
    ///
    /// The midpoint is the middle point between the current best bid and ask prices.
    ///
    /// # Arguments
    /// * `token_id` - The unique identifier for the token
    pub async fn get_midpoint(&self, token_id: &str) -> Result<MidpointResponse> {
        let url = format!("{}/midpoint", CLOB_API_BASE);
        let params = [("token_id", token_id)];

        let response = self.client.get(&url).query(&params).send().await?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::error::PolymarketError::InvalidData(error_text));
        }

        let midpoint: MidpointResponse = response.json().await?;
        Ok(midpoint)
    }

    /// Get historical price data for a token
    ///
    /// # Arguments
    /// * `token_id` - The CLOB token ID for which to fetch price history
    /// * `start_ts` - Optional start time as Unix timestamp in UTC
    /// * `end_ts` - Optional end time as Unix timestamp in UTC
    /// * `interval` - Optional interval string (mutually exclusive with start_ts/end_ts)
    /// * `fidelity` - Optional resolution of the data in minutes
    pub async fn get_prices_history(
        &self,
        token_id: &str,
        start_ts: Option<i64>,
        end_ts: Option<i64>,
        interval: Option<PriceInterval>,
        fidelity: Option<u32>,
    ) -> Result<PriceHistoryResponse> {
        let url = format!("{}/prices-history", CLOB_API_BASE);
        let mut params = vec![("market", token_id.to_string())];

        if let Some(start) = start_ts {
            params.push(("startTs", start.to_string()));
        }
        if let Some(end) = end_ts {
            params.push(("endTs", end.to_string()));
        }
        if let Some(interval) = interval {
            params.push(("interval", interval.as_str().to_string()));
        }
        if let Some(fidelity) = fidelity {
            params.push(("fidelity", fidelity.to_string()));
        }

        let response = self.client.get(&url).query(&params).send().await?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::error::PolymarketError::InvalidData(error_text));
        }

        let history: PriceHistoryResponse = response.json().await?;
        Ok(history)
    }

    /// Get bid-ask spreads for multiple tokens
    ///
    /// # Arguments
    /// * `requests` - Array of spread requests (max 500)
    pub async fn get_spreads(
        &self,
        requests: Vec<SpreadRequest>,
    ) -> Result<std::collections::HashMap<String, String>> {
        let url = format!("{}/spreads", CLOB_API_BASE);

        let response = self.client.post(&url).json(&requests).send().await?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::error::PolymarketError::InvalidData(error_text));
        }

        let spreads: std::collections::HashMap<String, String> = response.json().await?;
        Ok(spreads)
    }

    /// Get multiple orderbooks at once
    ///
    /// # Arguments
    /// * `requests` - Array of batch token requests (max 500)
    pub async fn get_orderbooks(&self, requests: Vec<BatchTokenRequest>) -> Result<Vec<Orderbook>> {
        let url = format!("{}/books", CLOB_API_BASE);

        let response = self.client.post(&url).json(&requests).send().await?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::error::PolymarketError::InvalidData(error_text));
        }

        let orderbooks: Vec<Orderbook> = response.json().await?;
        Ok(orderbooks)
    }

    /// Get multiple market prices at once (batch)
    ///
    /// # Arguments
    /// * `requests` - Array of batch token requests (max 500)
    ///
    /// # Returns
    /// HashMap mapping token_id to TokenPrices (buy/sell prices)
    pub async fn get_prices_batch(
        &self,
        requests: Vec<BatchTokenRequest>,
    ) -> Result<std::collections::HashMap<String, TokenPrices>> {
        let url = format!("{}/prices", CLOB_API_BASE);

        let response = self.client.post(&url).json(&requests).send().await?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(crate::error::PolymarketError::InvalidData(error_text));
        }

        let prices: std::collections::HashMap<String, TokenPrices> = response.json().await?;
        Ok(prices)
    }
}

impl Default for ClobClient {
    fn default() -> Self {
        Self::new()
    }
}
