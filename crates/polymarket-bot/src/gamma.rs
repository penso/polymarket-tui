use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};

const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

// Helper function to deserialize clobTokenIds which can be either a JSON string or an array
fn deserialize_clob_token_ids<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    // First try to deserialize as Option
    let opt: Option<serde_json::Value> = Option::deserialize(deserializer)?;

    let value = match opt {
        Some(v) => v,
        None => return Ok(None),
    };

    if value.is_null() {
        return Ok(None);
    }

    match value {
        serde_json::Value::String(s) => {
            // It's a JSON string, parse it
            serde_json::from_str(&s).map(Some).map_err(Error::custom)
        }
        serde_json::Value::Array(arr) => {
            // It's already an array, convert it
            Ok(Some(
                arr.into_iter()
                    .map(|v| {
                        if let serde_json::Value::String(s) = v {
                            s
                        } else {
                            v.to_string()
                        }
                    })
                    .collect(),
            ))
        }
        _ => Ok(None),
    }
}

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
    #[serde(
        rename = "clobTokenIds",
        deserialize_with = "deserialize_clob_token_ids",
        default
    )]
    pub clob_token_ids: Option<Vec<String>>,
    #[serde(default)]
    pub outcomes: String,
    #[serde(rename = "outcomePrices", default)]
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
                if let Some(token_ids) = market.clob_token_ids {
                    asset_ids.extend(token_ids);
                }
            }
        }

        Ok(asset_ids)
    }

    pub async fn get_market_info_by_asset_id(&self, asset_id: &str) -> Result<Option<MarketInfo>> {
        let events = self.get_active_events(Some(1000)).await?;

        for event in events {
            for market in event.markets {
                if let Some(ref token_ids) = market.clob_token_ids {
                    if token_ids.contains(&asset_id.to_string()) {
                        let outcomes: Vec<String> = if market.outcomes.is_empty() {
                            vec![]
                        } else {
                            serde_json::from_str(&market.outcomes).unwrap_or_default()
                        };
                        let prices: Vec<String> = if market.outcome_prices.is_empty() {
                            vec![]
                        } else {
                            serde_json::from_str(&market.outcome_prices).unwrap_or_default()
                        };

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
