//! CLOB (Central Limit Order Book) REST API client
//!
//! This module provides a client for interacting with Polymarket's CLOB REST API,
//! which allows fetching orderbooks, trades, and managing orders.

use {
    crate::error::Result,
    base64::{Engine, engine::general_purpose::STANDARD},
    hmac::{Hmac, Mac},
    reqwest::header::{HeaderMap, HeaderValue},
    serde::{Deserialize, Serialize},
    sha2::Sha256,
    std::time::{SystemTime, UNIX_EPOCH},
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

/// Open order from CLOB API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenOrder {
    pub id: String,
    pub status: String,
    pub owner: String,
    pub maker_address: String,
    pub market: String,
    pub asset_id: String,
    pub side: String,
    pub original_size: String,
    pub size_matched: String,
    pub price: String,
    #[serde(default)]
    pub associate_trades: Vec<String>,
    pub outcome: String,
    pub created_at: i64,
    #[serde(default)]
    pub expiration: Option<String>,
    #[serde(default)]
    pub order_type: Option<String>,
}

/// Balance and allowance response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceAllowance {
    pub balance: String,
    pub allowance: String,
}

/// Asset type for balance queries
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AssetType {
    Collateral,
    Conditional,
}

/// Cancel orders response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrdersResponse {
    pub canceled: Vec<String>,
    #[serde(default)]
    pub not_canceled: std::collections::HashMap<String, serde_json::Value>,
}

/// Order response from posting an order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub success: bool,
    #[serde(default)]
    pub error_msg: Option<String>,
    #[serde(default, rename = "orderID")]
    pub order_id: Option<String>,
    #[serde(default)]
    pub transactions_hashes: Vec<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub taking_amount: Option<String>,
    #[serde(default)]
    pub making_amount: Option<String>,
}

/// User order request for creating orders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOrderRequest {
    pub token_id: String,
    pub price: f64,
    pub size: f64,
    pub side: Side,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_rate_bps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration: Option<u64>,
}

/// Market order request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOrderRequest {
    pub token_id: String,
    pub amount: f64,
    pub side: Side,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_rate_bps: Option<u32>,
}

/// CLOB REST API client
pub struct ClobClient {
    client: reqwest::Client,
    api_key: Option<String>,
    api_secret: Option<String>,
    passphrase: Option<String>,
    /// Polygon wallet address (required for L2 authentication)
    address: Option<String>,
}

impl ClobClient {
    /// Create a new CLOB client without authentication (for public endpoints)
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: None,
            api_secret: None,
            passphrase: None,
            address: None,
        }
    }

    /// Create a new CLOB client with authentication
    pub fn with_auth(
        api_key: String,
        api_secret: String,
        passphrase: String,
        address: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: Some(api_key),
            api_secret: Some(api_secret),
            passphrase: Some(passphrase),
            address: Some(address),
        }
    }

    /// Create a new CLOB client from environment variables
    /// Requires: api_key, secret, passphrase, address (or poly_address)
    pub fn from_env() -> Self {
        let address = std::env::var("address")
            .or_else(|_| std::env::var("poly_address"))
            .or_else(|_| std::env::var("POLY_ADDRESS"))
            .ok();

        if let (Ok(api_key), Ok(api_secret), Ok(passphrase), Some(addr)) = (
            std::env::var("api_key"),
            std::env::var("secret"),
            std::env::var("passphrase"),
            address,
        ) {
            Self::with_auth(api_key, api_secret, passphrase, addr)
        } else {
            Self::new()
        }
    }

    /// Check if the client has authentication credentials
    pub fn has_auth(&self) -> bool {
        self.api_key.is_some()
            && self.api_secret.is_some()
            && self.passphrase.is_some()
            && self.address.is_some()
    }

    /// Create L2 authentication headers for a request
    fn create_l2_headers(
        &self,
        method: &str,
        request_path: &str,
        body: Option<&str>,
    ) -> Option<HeaderMap> {
        if let (Some(api_key), Some(secret), Some(passphrase), Some(address)) = (
            &self.api_key,
            &self.api_secret,
            &self.passphrase,
            &self.address,
        ) {
            match L2Headers::new(
                api_key,
                secret,
                passphrase,
                address,
                method,
                request_path,
                body,
            ) {
                Ok(headers) => Some(headers.to_header_map()),
                Err(e) => {
                    log_debug!("Failed to create L2 headers: {}", e);
                    None
                },
            }
        } else {
            None
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

    /// Get recent trades for a specific market with authentication
    /// Returns trade count if authenticated, otherwise returns an error
    pub async fn get_trades_authenticated(
        &self,
        market: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>> {
        // Build the query string
        let mut query_parts = vec![format!("market={}", market)];
        if let Some(limit) = limit {
            query_parts.push(format!("limit={}", limit));
        }
        let query_string = query_parts.join("&");
        let request_path = format!("/trades?{}", query_string);

        log_info!("GET {}{} (authenticated)", CLOB_API_BASE, request_path);

        // Create L2 auth headers
        let headers = self
            .create_l2_headers("GET", &request_path, None)
            .ok_or_else(|| {
                crate::error::PolymarketError::InvalidData(
                    "Missing authentication credentials".to_string(),
                )
            })?;

        let url = format!("{}{}", CLOB_API_BASE, request_path);
        let response = self.client.get(&url).headers(headers).send().await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            log_info!("GET {} -> error: {} - {}", request_path, status, error_text);
            return Err(crate::error::PolymarketError::InvalidData(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let trades: Vec<Trade> = response.json().await?;
        log_info!("GET {} -> {} trades", request_path, trades.len());
        Ok(trades)
    }

    /// Get trade count for a market (uses authenticated endpoint if credentials available)
    pub async fn get_trade_count(&self, market: &str) -> Result<usize> {
        if self.has_auth() {
            // Use authenticated endpoint to get all trades
            let trades = self.get_trades_authenticated(market, Some(1000)).await?;
            Ok(trades.len())
        } else {
            // Without auth, we can't get trade counts reliably
            Err(crate::error::PolymarketError::InvalidData(
                "Authentication required to fetch trade counts".to_string(),
            ))
        }
    }

    /// Get user's open orders (requires authentication)
    pub async fn get_open_orders(&self, market: Option<&str>) -> Result<Vec<OpenOrder>> {
        let request_path = if let Some(market) = market {
            format!("/orders?market={}", market)
        } else {
            "/orders".to_string()
        };

        let headers = self
            .create_l2_headers("GET", &request_path, None)
            .ok_or_else(|| {
                crate::error::PolymarketError::InvalidData(
                    "Missing authentication credentials".to_string(),
                )
            })?;

        let url = format!("{}{}", CLOB_API_BASE, request_path);
        let response = self.client.get(&url).headers(headers).send().await?;

        let status = response.status();
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

        let orders: Vec<OpenOrder> = response.json().await?;
        Ok(orders)
    }

    /// Get a specific order by ID (requires authentication)
    pub async fn get_order_by_id(&self, order_id: &str) -> Result<OpenOrder> {
        let request_path = format!("/orders/{}", order_id);

        let headers = self
            .create_l2_headers("GET", &request_path, None)
            .ok_or_else(|| {
                crate::error::PolymarketError::InvalidData(
                    "Missing authentication credentials".to_string(),
                )
            })?;

        let url = format!("{}{}", CLOB_API_BASE, request_path);
        let response = self.client.get(&url).headers(headers).send().await?;

        let status = response.status();
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

        let order: OpenOrder = response.json().await?;
        Ok(order)
    }

    /// Get balance and allowance for collateral (USDC)
    pub async fn get_balance_allowance(&self, asset_type: AssetType) -> Result<BalanceAllowance> {
        let asset_type_str = match asset_type {
            AssetType::Collateral => "COLLATERAL",
            AssetType::Conditional => "CONDITIONAL",
        };
        let request_path = format!("/balance-allowance?asset_type={}", asset_type_str);

        let headers = self
            .create_l2_headers("GET", &request_path, None)
            .ok_or_else(|| {
                crate::error::PolymarketError::InvalidData(
                    "Missing authentication credentials".to_string(),
                )
            })?;

        let url = format!("{}{}", CLOB_API_BASE, request_path);
        let response = self.client.get(&url).headers(headers).send().await?;

        let status = response.status();
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

        let balance: BalanceAllowance = response.json().await?;
        Ok(balance)
    }

    /// Cancel a single order (requires authentication)
    pub async fn cancel_order(&self, order_id: &str) -> Result<CancelOrdersResponse> {
        let request_path = format!("/orders/{}", order_id);

        let headers = self
            .create_l2_headers("DELETE", &request_path, None)
            .ok_or_else(|| {
                crate::error::PolymarketError::InvalidData(
                    "Missing authentication credentials".to_string(),
                )
            })?;

        let url = format!("{}{}", CLOB_API_BASE, request_path);
        let response = self.client.delete(&url).headers(headers).send().await?;

        let status = response.status();
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

        let result: CancelOrdersResponse = response.json().await?;
        Ok(result)
    }

    /// Cancel all open orders (requires authentication)
    pub async fn cancel_all_orders(&self) -> Result<CancelOrdersResponse> {
        let request_path = "/orders";

        let headers = self
            .create_l2_headers("DELETE", request_path, None)
            .ok_or_else(|| {
                crate::error::PolymarketError::InvalidData(
                    "Missing authentication credentials".to_string(),
                )
            })?;

        let url = format!("{}{}", CLOB_API_BASE, request_path);
        let response = self.client.delete(&url).headers(headers).send().await?;

        let status = response.status();
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

        let result: CancelOrdersResponse = response.json().await?;
        Ok(result)
    }

    /// Get the wallet address used for authentication
    pub fn get_address(&self) -> Option<&str> {
        self.address.as_deref()
    }

    /// Get the API key (for display purposes)
    pub fn get_api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
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

type HmacSha256 = Hmac<Sha256>;

/// Build HMAC-SHA256 signature for L2 authentication
///
/// The signature is created by concatenating: timestamp + method + requestPath + body (optional)
/// Then signing with HMAC-SHA256 using the base64-decoded secret.
fn build_hmac_signature(
    secret: &str,
    timestamp: i64,
    method: &str,
    request_path: &str,
    body: Option<&str>,
) -> std::result::Result<String, String> {
    // Decode the base64 standard secret
    let secret_bytes = STANDARD
        .decode(secret)
        .map_err(|e| format!("Failed to decode secret: {}", e))?;

    // Build the message: timestamp + method + requestPath + body
    let mut message = format!("{}{}{}", timestamp, method, request_path);
    if let Some(body) = body {
        message.push_str(body);
    }

    // Create HMAC-SHA256
    let mut mac = HmacSha256::new_from_slice(&secret_bytes)
        .map_err(|e| format!("Failed to create HMAC: {}", e))?;
    mac.update(message.as_bytes());

    // Get the signature and encode as base64 standard
    let result = mac.finalize();
    let signature = STANDARD.encode(result.into_bytes());

    Ok(signature)
}

/// L2 authentication headers for CLOB API requests
pub struct L2Headers {
    pub api_key: String,
    pub signature: String,
    pub timestamp: i64,
    pub passphrase: String,
    pub address: String,
}

impl L2Headers {
    /// Create L2 authentication headers
    pub fn new(
        api_key: &str,
        secret: &str,
        passphrase: &str,
        address: &str,
        method: &str,
        request_path: &str,
        body: Option<&str>,
    ) -> std::result::Result<Self, String> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("Failed to get timestamp: {}", e))?
            .as_secs() as i64;

        let signature = build_hmac_signature(secret, timestamp, method, request_path, body)?;

        Ok(Self {
            api_key: api_key.to_string(),
            signature,
            timestamp,
            passphrase: passphrase.to_string(),
            address: address.to_string(),
        })
    }

    /// Convert to reqwest HeaderMap
    /// Uses underscore format as per Polymarket API spec: POLY_API_KEY, POLY_SIGNATURE, etc.
    pub fn to_header_map(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "POLY_ADDRESS",
            HeaderValue::from_str(&self.address).unwrap_or_else(|_| HeaderValue::from_static("")),
        );
        headers.insert(
            "POLY_API_KEY",
            HeaderValue::from_str(&self.api_key).unwrap_or_else(|_| HeaderValue::from_static("")),
        );
        headers.insert(
            "POLY_SIGNATURE",
            HeaderValue::from_str(&self.signature).unwrap_or_else(|_| HeaderValue::from_static("")),
        );
        headers.insert(
            "POLY_TIMESTAMP",
            HeaderValue::from_str(&self.timestamp.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        );
        headers.insert(
            "POLY_PASSPHRASE",
            HeaderValue::from_str(&self.passphrase)
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        );
        headers
    }
}
