//! Data API client
//!
//! This module provides a client for interacting with Polymarket's Data API,
//! which allows querying user positions, trade history, and portfolio data.

use crate::error::Result;
use serde::{Deserialize, Serialize};

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

/// User position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub asset: String,
    pub condition_id: String,
    pub quantity: String,
    pub value: Option<String>,
    pub title: String,
    pub slug: String,
    pub icon: Option<String>,
    pub event_slug: String,
    pub outcome: String,
    pub outcome_index: i32,
}

/// Portfolio summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    pub total_value: Option<String>,
    pub positions: Vec<Position>,
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
        let mut params = vec![
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
        let mut params = vec![
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
}

impl Default for DataClient {
    fn default() -> Self {
        Self::new()
    }
}

