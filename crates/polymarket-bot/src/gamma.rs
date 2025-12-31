use crate::cache::FileCache;
use crate::error::Result;
use serde::{Deserialize, Deserializer, Serialize};

const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

// Helper function to deserialize clobTokenIds which can be either a JSON string or an array
fn deserialize_clob_token_ids<'de, D>(deserializer: D) -> std::result::Result<Option<Vec<String>>, D::Error>
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
    cache: Option<FileCache>,
}

impl GammaClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: None,
        }
    }

    /// Create a new GammaClient with file-based caching
    pub fn with_cache<P: AsRef<std::path::Path>>(cache_dir: P) -> Result<Self> {
        let cache = FileCache::new(cache_dir)?;
        Ok(Self {
            client: reqwest::Client::new(),
            cache: Some(cache),
        })
    }

    /// Set cache TTL (time to live) in seconds
    pub fn set_cache_ttl(&mut self, ttl_seconds: u64) -> Result<()> {
        if let Some(ref mut cache) = self.cache {
            *cache = cache.clone().with_default_ttl(ttl_seconds);
        }
        Ok(())
    }

    /// Set cache for this client
    pub fn set_cache(&mut self, cache: FileCache) {
        self.cache = Some(cache);
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

    /// Get event by ID
    pub async fn get_event_by_id(&self, event_id: &str) -> Result<Option<Event>> {
        let url = format!("{}/events/{}", GAMMA_API_BASE, event_id);
        let response = self.client.get(&url).send().await?;
        
        if response.status() == 404 {
            return Ok(None);
        }
        
        let event: Event = response.json().await?;
        Ok(Some(event))
    }

    /// Get event by slug
    pub async fn get_event_by_slug(&self, slug: &str) -> Result<Option<Event>> {
        let url = format!("{}/events?slug={}", GAMMA_API_BASE, slug);
        let events: Vec<Event> = self.client.get(&url).send().await?.json().await?;
        Ok(events.into_iter().next())
    }

    /// Get market by ID
    pub async fn get_market_by_id(&self, market_id: &str) -> Result<Option<Market>> {
        let url = format!("{}/markets/{}", GAMMA_API_BASE, market_id);
        let response = self.client.get(&url).send().await?;
        
        if response.status() == 404 {
            return Ok(None);
        }
        
        let market: Market = response.json().await?;
        Ok(Some(market))
    }

    /// Get all markets (with optional filters)
    pub async fn get_markets(
        &self,
        active: Option<bool>,
        closed: Option<bool>,
        limit: Option<usize>,
    ) -> Result<Vec<Market>> {
        let mut url = format!("{}/markets", GAMMA_API_BASE);
        let mut params = Vec::new();
        
        if let Some(active) = active {
            params.push(("active", active.to_string()));
        }
        if let Some(closed) = closed {
            params.push(("closed", closed.to_string()));
        }
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        
        let markets: Vec<Market> = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(markets)
    }

    /// Get categories/tags
    pub async fn get_categories(&self) -> Result<Vec<Tag>> {
        let url = format!("{}/categories", GAMMA_API_BASE);
        let categories: Vec<Tag> = self.client.get(&url).send().await?.json().await?;
        Ok(categories)
    }

    /// Get events by category/tag
    pub async fn get_events_by_category(
        &self,
        category_slug: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Event>> {
        let limit = limit.unwrap_or(100);
        let url = format!(
            "{}/events?category={}&limit={}",
            GAMMA_API_BASE, category_slug, limit
        );
        let events: Vec<Event> = self.client.get(&url).send().await?.json().await?;
        Ok(events)
    }

    pub async fn get_market_info_by_asset_id(&self, asset_id: &str) -> Result<Option<MarketInfo>> {
        // Check cache first
        if let Some(ref cache) = self.cache {
            let cache_key = format!("market_info_{}", asset_id);
            if let Some(cached_info) = cache.get::<MarketInfo>(&cache_key)? {
                return Ok(Some(cached_info));
            }
        }

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

                        let market_info = MarketInfo {
                            event_title: event.title,
                            event_slug: event.slug,
                            market_question: market.question,
                            market_id: market.id,
                            asset_id: asset_id.to_string(),
                            outcomes,
                            prices,
                        };

                        // Cache the result
                        if let Some(ref cache) = self.cache {
                            let cache_key = format!("market_info_{}", asset_id);
                            let _ = cache.set(&cache_key, &market_info);
                        }

                        return Ok(Some(market_info));
                    }
                }
            }
        }

        Ok(None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
