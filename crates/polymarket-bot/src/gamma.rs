use anyhow::Result;
use serde::{Deserialize, Serialize};

const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub active: bool,
    pub closed: bool,
    pub tags: Vec<Tag>,
    pub markets: Vec<Market>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub label: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: String,
    pub question: String,
    #[serde(rename = "clobTokenIds")]
    pub clob_token_ids: Vec<String>,
    pub outcomes: String,
    #[serde(rename = "outcomePrices")]
    pub outcome_prices: String,
}

pub struct GammaClient {
    client: reqwest::Client,
}

impl GammaClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_active_events(&self, limit: Option<usize>) -> Result<Vec<Event>> {
        let limit = limit.unwrap_or(100);
        let url = format!(
            "{}/events?active=true&closed=false&limit={}",
            GAMMA_API_BASE, limit
        );
        let events: Vec<Event> = self.client.get(&url).send().await?.json().await?;
        Ok(events)
    }

    pub async fn get_market_by_slug(&self, slug: &str) -> Result<Vec<Market>> {
        let url = format!("{}/markets?slug={}", GAMMA_API_BASE, slug);
        let response: serde_json::Value = self.client.get(&url).send().await?.json().await?;

        // The API might return a single market or an array
        let markets = if response.is_array() {
            serde_json::from_value(response)?
        } else {
            vec![serde_json::from_value(response)?]
        };

        Ok(markets)
    }

    pub async fn get_all_active_asset_ids(&self) -> Result<Vec<String>> {
        let events = self.get_active_events(None).await?;
        let mut asset_ids = Vec::new();

        for event in events {
            for market in event.markets {
                asset_ids.extend(market.clob_token_ids);
            }
        }

        Ok(asset_ids)
    }

    pub async fn get_market_info_by_asset_id(&self, asset_id: &str) -> Result<Option<MarketInfo>> {
        let events = self.get_active_events(Some(1000)).await?;

        for event in events {
            for market in event.markets {
                if market.clob_token_ids.contains(&asset_id.to_string()) {
                    let outcomes: Vec<String> = serde_json::from_str(&market.outcomes)?;
                    let prices: Vec<String> = serde_json::from_str(&market.outcome_prices)?;

                    return Ok(Some(MarketInfo {
                        event_title: event.title,
                        event_slug: event.slug,
                        market_question: market.question,
                        market_id: market.id,
                        asset_id: asset_id.to_string(),
                        outcomes,
                        prices,
                    }));
                }
            }
        }

        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct MarketInfo {
    pub event_title: String,
    pub event_slug: String,
    pub market_question: String,
    pub market_id: String,
    pub asset_id: String,
    pub outcomes: Vec<String>,
    pub prices: Vec<String>,
}

impl Default for GammaClient {
    fn default() -> Self {
        Self::new()
    }
}
