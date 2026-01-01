use {
    crate::{cache::FileCache, error::Result},
    serde::{Deserialize, Deserializer, Serialize},
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

/// Macro for conditional warn logging based on tracing feature
#[cfg(feature = "tracing")]
macro_rules! log_warn {
    ($($arg:tt)*) => { tracing::warn!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_warn {
    ($($arg:tt)*) => {};
}

/// Macro for conditional error logging based on tracing feature
#[cfg(feature = "tracing")]
macro_rules! log_error {
    ($($arg:tt)*) => { tracing::error!($($arg)*) };
}

#[cfg(not(feature = "tracing"))]
macro_rules! log_error {
    ($($arg:tt)*) => {};
}

const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

// Helper function to deserialize clobTokenIds which can be either a JSON string or an array
fn deserialize_clob_token_ids<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Vec<String>>, D::Error>
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
        },
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
        },
        _ => Ok(None),
    }
}

// Helper function to deserialize outcomes/outcomePrices which can be either a JSON string or an array
fn deserialize_string_array<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;

    match value {
        serde_json::Value::String(s) => {
            // It's a JSON string, parse it
            serde_json::from_str(&s).map_err(Error::custom)
        },
        serde_json::Value::Array(arr) => {
            // It's already an array, convert it
            Ok(arr
                .into_iter()
                .map(|v| {
                    if let serde_json::Value::String(s) = v {
                        s
                    } else {
                        v.to_string()
                    }
                })
                .collect())
        },
        _ => Ok(vec![]),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub active: bool,
    pub closed: bool,
    #[serde(default)]
    pub tags: Vec<Tag>,
    pub markets: Vec<Market>,
    #[serde(rename = "endDate", default)]
    pub end_date: Option<String>, // ISO 8601 date string
    #[serde(default)]
    pub image: Option<String>, // URL to event image/thumbnail
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub label: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    #[serde(default)]
    pub id: Option<String>,
    pub question: String,
    #[serde(
        rename = "clobTokenIds",
        deserialize_with = "deserialize_clob_token_ids",
        default
    )]
    pub clob_token_ids: Option<Vec<String>>,
    #[serde(deserialize_with = "deserialize_string_array", default)]
    pub outcomes: Vec<String>,
    #[serde(
        rename = "outcomePrices",
        deserialize_with = "deserialize_string_array",
        default
    )]
    pub outcome_prices: Vec<String>,
    #[serde(rename = "volume24hr", default)]
    pub volume_24hr: Option<f64>,
    #[serde(rename = "volumeTotal", default)]
    pub volume_total: Option<f64>,
    /// Whether the market is active (accepting new trades)
    #[serde(default)]
    pub active: bool,
    /// Whether the market has been closed/resolved
    #[serde(default)]
    pub closed: bool,
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

    /// Get trending events ordered by trading volume
    ///
    /// # Arguments
    /// * `order_by` - Field to order by (e.g., "volume24hr", "volume7d", "volume30d")
    /// * `ascending` - If true, sort ascending; if false, sort descending
    /// * `limit` - Maximum number of events to return
    pub async fn get_trending_events(
        &self,
        order_by: Option<&str>,
        ascending: Option<bool>,
        limit: Option<usize>,
    ) -> Result<Vec<Event>> {
        let limit = limit.unwrap_or(50);
        let order_by = order_by.unwrap_or("volume24hr");
        let ascending = ascending.unwrap_or(false);

        let url = format!(
            "{}/events?active=true&closed=false&order={}&ascending={}&limit={}",
            GAMMA_API_BASE, order_by, ascending, limit
        );

        log_info!("GET {}", url);

        let response = self.client.get(&url).send().await?;
        let _status = response.status();

        log_info!("GET {} -> status: {}", url, _status);

        let events: Vec<Event> = response.json().await?;
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
        let url = format!("{}/markets", GAMMA_API_BASE);
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

    /// Search events by query string using the public-search endpoint
    pub async fn search_events(&self, query: &str, limit: Option<usize>) -> Result<Vec<Event>> {
        let limit_per_type = limit.unwrap_or(50);
        let url = format!(
            "{}/public-search?q={}&optimized=true&limit_per_type={}&type=events&search_tags=true&search_profiles=true&cache=true",
            GAMMA_API_BASE,
            urlencoding::encode(query),
            limit_per_type
        );

        // Log the API call
        log_info!("GET {}", url);

        let response = self.client.get(&url).send().await.inspect_err(|_e| {
            log_error!("Failed to send search request: {}", _e);
        })?;

        let status = response.status();
        log_info!("GET {} -> status: {}", url, status);

        let response_text = response.text().await.inspect_err(|_e| {
            log_error!("Failed to read search response body: {}", _e);
        })?;

        // Only log response body on error or in debug mode
        if !status.is_success() {
            log_debug!(
                "Search API response body (first 500 chars): {}",
                if response_text.len() > 500 {
                    &response_text[..500]
                } else {
                    &response_text
                }
            );
        }

        if !status.is_success() {
            log_warn!(
                "Search API error: status={}, body={}",
                status,
                response_text
            );
            return Err(crate::error::PolymarketError::InvalidData(format!(
                "Search API returned status {}: {}",
                status, response_text
            )));
        }

        #[derive(Deserialize)]
        struct SearchResponse {
            events: Vec<Event>,
            #[allow(dead_code)]
            profiles: Option<serde_json::Value>,
            #[allow(dead_code)]
            tags: Option<serde_json::Value>,
            #[allow(dead_code)]
            has_more: Option<bool>,
        }

        let search_response: SearchResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                log_error!(
                    "Failed to parse search response: {}, body (first 1000 chars): {}",
                    e,
                    if response_text.len() > 1000 {
                        &response_text[..1000]
                    } else {
                        &response_text
                    }
                );
                crate::error::PolymarketError::Serialization(e)
            })?;

        log_info!("Search returned {} events", search_response.events.len());

        Ok(search_response.events)
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
                if let Some(ref token_ids) = market.clob_token_ids
                    && token_ids.contains(&asset_id.to_string())
                {
                    let outcomes = market.outcomes.clone();
                    let prices = market.outcome_prices.clone();

                    let market_info = MarketInfo {
                        event_title: event.title,
                        event_slug: event.slug,
                        market_question: market.question,
                        market_id: market.id.clone().unwrap_or_default(),
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
