//! Data API client
//!
//! This module provides a client for interacting with Polymarket's Data API,
//! which allows querying user positions, trade history, and portfolio data.

use {
    crate::error::Result,
    serde::{Deserialize, Serialize},
};

const DATA_API_BASE: &str = "https://data-api.polymarket.com";

/// Trade information from Data API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTrade {
    pub proxy_wallet: String,
    pub side: String,
    pub asset: String,
    pub condition_id: String,
    pub size: f64,
    pub price: f64,
    pub timestamp: i64,
    pub title: String,
    pub slug: String,
    pub icon: Option<String>,
    pub event_slug: String,
    pub outcome: String,
    pub outcome_index: i32,
    pub name: String,
    pub pseudonym: String,
    pub bio: Option<String>,
    pub profile_image: Option<String>,
    pub profile_image_optimized: Option<String>,
    pub transaction_hash: String,
}

/// User position with comprehensive fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    #[serde(rename = "proxyWallet", default)]
    pub proxy_wallet: Option<String>,
    pub asset: String,
    #[serde(rename = "conditionId", alias = "condition_id")]
    pub condition_id: String,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(rename = "avgPrice", default)]
    pub avg_price: Option<f64>,
    #[serde(rename = "initialValue", default)]
    pub initial_value: Option<f64>,
    #[serde(rename = "currentValue", default)]
    pub current_value: Option<f64>,
    #[serde(rename = "cashPnl", default)]
    pub cash_pnl: Option<f64>,
    #[serde(rename = "percentPnl", default)]
    pub percent_pnl: Option<f64>,
    #[serde(rename = "totalBought", default)]
    pub total_bought: Option<f64>,
    #[serde(rename = "realizedPnl", default)]
    pub realized_pnl: Option<f64>,
    #[serde(rename = "percentRealizedPnl", default)]
    pub percent_realized_pnl: Option<f64>,
    #[serde(rename = "curPrice", default)]
    pub cur_price: Option<f64>,
    #[serde(default)]
    pub redeemable: Option<bool>,
    #[serde(default)]
    pub mergeable: Option<bool>,
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(rename = "eventSlug", alias = "event_slug")]
    pub event_slug: String,
    pub outcome: String,
    #[serde(rename = "outcomeIndex", alias = "outcome_index")]
    pub outcome_index: i32,
    #[serde(rename = "oppositeOutcome", default)]
    pub opposite_outcome: Option<String>,
    #[serde(rename = "oppositeAsset", default)]
    pub opposite_asset: Option<String>,
    #[serde(rename = "endDate", default)]
    pub end_date: Option<String>,
    #[serde(rename = "negativeRisk", default)]
    pub negative_risk: Option<bool>,
    // Legacy field for backwards compatibility
    #[serde(default)]
    pub quantity: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
}

/// Portfolio summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    pub total_value: Option<String>,
    pub positions: Vec<Position>,
}

/// Activity type enum
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum ActivityType {
    Trade,
    Split,
    Merge,
    Redeem,
    Reward,
    Conversion,
}

impl ActivityType {
    fn as_str(&self) -> &'static str {
        match self {
            ActivityType::Trade => "TRADE",
            ActivityType::Split => "SPLIT",
            ActivityType::Merge => "MERGE",
            ActivityType::Redeem => "REDEEM",
            ActivityType::Reward => "REWARD",
            ActivityType::Conversion => "CONVERSION",
        }
    }
}

/// Sort field for activity queries
#[derive(Debug, Clone, Copy)]
pub enum ActivitySortBy {
    Timestamp,
    Tokens,
    Cash,
}

impl ActivitySortBy {
    fn as_str(&self) -> &'static str {
        match self {
            ActivitySortBy::Timestamp => "TIMESTAMP",
            ActivitySortBy::Tokens => "TOKENS",
            ActivitySortBy::Cash => "CASH",
        }
    }
}

/// Sort direction
#[derive(Debug, Clone, Copy)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    fn as_str(&self) -> &'static str {
        match self {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        }
    }
}

/// User activity record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    #[serde(rename = "proxyWallet")]
    pub proxy_wallet: String,
    pub timestamp: i64,
    #[serde(rename = "conditionId")]
    pub condition_id: String,
    #[serde(rename = "type")]
    pub activity_type: ActivityType,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(rename = "usdcSize", default)]
    pub usdc_size: Option<f64>,
    #[serde(rename = "transactionHash")]
    pub transaction_hash: String,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(rename = "outcomeIndex", default)]
    pub outcome_index: Option<i32>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(rename = "eventSlug", default)]
    pub event_slug: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub pseudonym: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(rename = "profileImage", default)]
    pub profile_image: Option<String>,
    #[serde(rename = "profileImageOptimized", default)]
    pub profile_image_optimized: Option<String>,
}

/// Trade side filter
#[derive(Debug, Clone, Copy)]
pub enum TradeSide {
    Buy,
    Sell,
}

impl TradeSide {
    fn as_str(&self) -> &'static str {
        match self {
            TradeSide::Buy => "BUY",
            TradeSide::Sell => "SELL",
        }
    }
}

/// Data API client
pub struct DataClient {
    client: reqwest::Client,
}

impl DataClient {
    /// Create a new Data API client
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Get trades for a specific event
    pub async fn get_trades_by_event(
        &self,
        event_id: u64,
        limit: Option<usize>,
        offset: Option<usize>,
        filter_type: Option<&str>,
        filter_amount: Option<f64>,
    ) -> Result<Vec<DataTrade>> {
        let url = format!("{}/trades", DATA_API_BASE);
        let mut params = vec![
            ("eventId", event_id.to_string()),
            ("limit", limit.unwrap_or(10).to_string()),
            ("offset", offset.unwrap_or(0).to_string()),
        ];

        if let Some(filter_type) = filter_type {
            params.push(("filterType", filter_type.to_string()));
        }
        if let Some(filter_amount) = filter_amount {
            params.push(("filterAmount", filter_amount.to_string()));
        }

        let trades: Vec<DataTrade> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(trades)
    }

    /// Get trades for a specific event slug
    pub async fn get_trades_by_event_slug(
        &self,
        event_slug: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<DataTrade>> {
        let url = format!("{}/trades", DATA_API_BASE);
        let params = vec![
            ("eventSlug", event_slug.to_string()),
            ("limit", limit.unwrap_or(10).to_string()),
            ("offset", offset.unwrap_or(0).to_string()),
        ];

        let trades: Vec<DataTrade> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(trades)
    }

    /// Get trades for a specific market (condition ID)
    pub async fn get_trades_by_market(
        &self,
        condition_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<DataTrade>> {
        let url = format!("{}/trades", DATA_API_BASE);
        let params = vec![
            ("conditionId", condition_id.to_string()),
            ("limit", limit.unwrap_or(10).to_string()),
            ("offset", offset.unwrap_or(0).to_string()),
        ];

        let trades: Vec<DataTrade> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(trades)
    }

    /// Get user positions (requires authentication)
    pub async fn get_positions(&self, user_address: &str) -> Result<Vec<Position>> {
        let url = format!("{}/positions", DATA_API_BASE);
        let params = [("user", user_address)];
        let positions: Vec<Position> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(positions)
    }

    /// Get portfolio for a user (requires authentication)
    pub async fn get_portfolio(&self, user_address: &str) -> Result<Portfolio> {
        let url = format!("{}/portfolio", DATA_API_BASE);
        let params = [("user", user_address)];
        let portfolio: Portfolio = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(portfolio)
    }

    /// Get user activity (trades, splits, merges, redeems, rewards, conversions)
    ///
    /// # Arguments
    /// * `user_address` - The user's wallet address (0x-prefixed)
    /// * `limit` - Results per page (0-500, default 100)
    /// * `offset` - Pagination offset (0-10000, default 0)
    /// * `market` - Optional comma-separated condition IDs
    /// * `event_id` - Optional event ID (mutually exclusive with market)
    /// * `activity_types` - Optional filter by activity types
    /// * `start` - Optional start timestamp
    /// * `end` - Optional end timestamp
    /// * `sort_by` - Optional sort field (default: TIMESTAMP)
    /// * `sort_direction` - Optional sort direction (default: DESC)
    /// * `side` - Optional trade side filter
    #[allow(clippy::too_many_arguments)]
    pub async fn get_activity(
        &self,
        user_address: &str,
        limit: Option<usize>,
        offset: Option<usize>,
        market: Option<&str>,
        event_id: Option<u64>,
        activity_types: Option<Vec<ActivityType>>,
        start: Option<i64>,
        end: Option<i64>,
        sort_by: Option<ActivitySortBy>,
        sort_direction: Option<SortDirection>,
        side: Option<TradeSide>,
    ) -> Result<Vec<Activity>> {
        let url = format!("{}/activity", DATA_API_BASE);
        let mut params = vec![
            ("user", user_address.to_string()),
            ("limit", limit.unwrap_or(100).to_string()),
            ("offset", offset.unwrap_or(0).to_string()),
        ];

        if let Some(market) = market {
            params.push(("market", market.to_string()));
        }
        if let Some(event_id) = event_id {
            params.push(("eventId", event_id.to_string()));
        }
        if let Some(types) = activity_types {
            let type_str = types
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(",");
            params.push(("type", type_str));
        }
        if let Some(start) = start {
            params.push(("start", start.to_string()));
        }
        if let Some(end) = end {
            params.push(("end", end.to_string()));
        }
        if let Some(sort_by) = sort_by {
            params.push(("sortBy", sort_by.as_str().to_string()));
        }
        if let Some(sort_direction) = sort_direction {
            params.push(("sortDirection", sort_direction.as_str().to_string()));
        }
        if let Some(side) = side {
            params.push(("side", side.as_str().to_string()));
        }

        let activities: Vec<Activity> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(activities)
    }

    /// Get trades with enhanced filtering options
    ///
    /// # Arguments
    /// * `user_address` - Optional user wallet address
    /// * `market` - Optional comma-separated condition IDs
    /// * `event_id` - Optional event ID (mutually exclusive with market)
    /// * `limit` - Results per page (0-10000, default 100)
    /// * `offset` - Pagination offset (0-10000, default 0)
    /// * `taker_only` - Filter to taker-initiated trades (default true)
    /// * `filter_type` - Optional filter type (CASH or TOKENS)
    /// * `filter_amount` - Optional filter amount
    /// * `side` - Optional trade side filter
    #[allow(clippy::too_many_arguments)]
    pub async fn get_trades(
        &self,
        user_address: Option<&str>,
        market: Option<&str>,
        event_id: Option<u64>,
        limit: Option<usize>,
        offset: Option<usize>,
        taker_only: Option<bool>,
        filter_type: Option<&str>,
        filter_amount: Option<f64>,
        side: Option<TradeSide>,
    ) -> Result<Vec<DataTrade>> {
        let url = format!("{}/trades", DATA_API_BASE);
        let mut params = vec![
            ("limit", limit.unwrap_or(100).to_string()),
            ("offset", offset.unwrap_or(0).to_string()),
        ];

        if let Some(user) = user_address {
            params.push(("user", user.to_string()));
        }
        if let Some(market) = market {
            params.push(("market", market.to_string()));
        }
        if let Some(event_id) = event_id {
            params.push(("eventId", event_id.to_string()));
        }
        if let Some(taker_only) = taker_only {
            params.push(("takerOnly", taker_only.to_string()));
        }
        if let Some(filter_type) = filter_type {
            params.push(("filterType", filter_type.to_string()));
        }
        if let Some(filter_amount) = filter_amount {
            params.push(("filterAmount", filter_amount.to_string()));
        }
        if let Some(side) = side {
            params.push(("side", side.as_str().to_string()));
        }

        let trades: Vec<DataTrade> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(trades)
    }

    /// Get positions with enhanced filtering options
    ///
    /// # Arguments
    /// * `user_address` - User wallet address
    /// * `market` - Optional comma-separated condition IDs
    /// * `event_id` - Optional event ID
    /// * `size_threshold` - Minimum position size (default 1)
    /// * `redeemable` - Filter redeemable positions
    /// * `mergeable` - Filter mergeable positions
    /// * `limit` - Results per page (0-500, default 100)
    /// * `offset` - Pagination offset (0-10000, default 0)
    #[allow(clippy::too_many_arguments)]
    pub async fn get_positions_filtered(
        &self,
        user_address: &str,
        market: Option<&str>,
        event_id: Option<u64>,
        size_threshold: Option<f64>,
        redeemable: Option<bool>,
        mergeable: Option<bool>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<Position>> {
        let url = format!("{}/positions", DATA_API_BASE);
        let mut params = vec![
            ("user", user_address.to_string()),
            ("limit", limit.unwrap_or(100).to_string()),
            ("offset", offset.unwrap_or(0).to_string()),
        ];

        if let Some(market) = market {
            params.push(("market", market.to_string()));
        }
        if let Some(event_id) = event_id {
            params.push(("eventId", event_id.to_string()));
        }
        if let Some(size_threshold) = size_threshold {
            params.push(("sizeThreshold", size_threshold.to_string()));
        }
        if let Some(redeemable) = redeemable {
            params.push(("redeemable", redeemable.to_string()));
        }
        if let Some(mergeable) = mergeable {
            params.push(("mergeable", mergeable.to_string()));
        }

        let positions: Vec<Position> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(positions)
    }
}

impl Default for DataClient {
    fn default() -> Self {
        Self::new()
    }
}
