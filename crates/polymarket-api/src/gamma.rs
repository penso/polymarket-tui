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
    #[serde(default)]
    pub markets: Vec<Market>,
    #[serde(rename = "endDate", default)]
    pub end_date: Option<String>, // ISO 8601 date string
    #[serde(default)]
    pub image: Option<String>, // URL to event image/thumbnail
    /// Volume in last 24 hours (from favorites API)
    #[serde(rename = "volume24hr", default)]
    pub volume_24hr: Option<f64>,
    /// Total volume
    #[serde(default)]
    pub volume: Option<f64>,
    /// Total liquidity
    #[serde(default)]
    pub liquidity: Option<f64>,
    /// Competitive score (0-1, higher means more competitive/closer odds)
    #[serde(default)]
    pub competitive: Option<f64>,
    /// When the event was created
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
    /// Max price change in the last 24 hours across all markets (for Breaking tab)
    /// This is populated when fetching breaking events, not from the API directly
    #[serde(skip)]
    pub max_price_change_24hr: Option<f64>,
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
    /// Short display name for grouped markets (e.g., "400-419" for tweet count ranges)
    #[serde(rename = "groupItemTitle", default)]
    pub group_item_title: Option<String>,
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
    /// Market slug for URL construction
    #[serde(default)]
    pub slug: Option<String>,
    /// Whether the market is accepting orders
    #[serde(rename = "acceptingOrders", default)]
    pub accepting_orders: bool,
    /// UMA oracle resolution statuses (JSON string like "[\"proposed\", \"disputed\"]")
    #[serde(rename = "umaResolutionStatuses", default)]
    pub uma_resolution_statuses: Option<String>,
    /// Events this market belongs to (always 0 or 1 element)
    #[serde(default)]
    pub events: Vec<MarketEventRef>,
    /// Price change in the last 24 hours (used for Breaking tab sorting)
    #[serde(rename = "oneDayPriceChange", default)]
    pub one_day_price_change: Option<f64>,
}

impl Market {
    /// Get the event this market belongs to (markets have at most one event)
    pub fn event(&self) -> Option<&MarketEventRef> {
        self.events.first()
    }

    /// Check if market is in resolution/review process
    pub fn is_in_review(&self) -> bool {
        if let Some(ref statuses) = self.uma_resolution_statuses {
            statuses.contains("proposed") || statuses.contains("disputed")
        } else {
            false
        }
    }

    /// Get human-readable status string
    pub fn status(&self) -> &'static str {
        if self.closed {
            "closed"
        } else if self.is_in_review() {
            "in-review"
        } else if self.active {
            "open"
        } else {
            "paused"
        }
    }
}

/// Lightweight event reference embedded in market responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEventRef {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(rename = "endDate")]
    pub end_date: Option<String>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub closed: bool,
}

impl MarketEventRef {
    /// Get human-readable status string
    pub fn status(&self) -> &'static str {
        if self.closed {
            "closed"
        } else if self.active {
            "active"
        } else {
            "inactive"
        }
    }
}

pub struct GammaClient {
    client: reqwest::Client,
    cache: Option<FileCache>,
    /// Authentication credentials (for favorite events, etc.)
    auth: Option<GammaAuth>,
}

/// Authentication credentials for Gamma API
///
/// For cookie-based auth (favorites, etc.), session cookies are needed.
/// These can be obtained from browser dev tools after logging in to polymarket.com.
#[derive(Debug, Clone)]
pub struct GammaAuth {
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub address: String,
    /// Session cookie for browser-based authentication (polymarketsession)
    /// Required for endpoints like favorite_events that don't support HMAC auth
    pub session_cookie: Option<String>,
    /// Session nonce cookie (polymarketnonce)
    pub session_nonce: Option<String>,
    /// Auth type cookie (polymarketauthtype), usually "magic"
    pub session_auth_type: Option<String>,
}

impl GammaClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: None,
            auth: None,
        }
    }

    /// Create a new GammaClient with authentication
    pub fn with_auth(auth: GammaAuth) -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: None,
            auth: Some(auth),
        }
    }

    /// Set authentication credentials
    pub fn set_auth(&mut self, auth: GammaAuth) {
        self.auth = Some(auth);
    }

    /// Check if the client has authentication credentials
    pub fn has_auth(&self) -> bool {
        self.auth.is_some()
    }

    /// Check if the client has a session cookie for browser-based auth
    pub fn has_session_cookie(&self) -> bool {
        self.auth
            .as_ref()
            .and_then(|a| a.session_cookie.as_ref())
            .is_some_and(|s| !s.is_empty())
    }

    /// Create a new GammaClient with file-based caching
    pub fn with_cache<P: AsRef<std::path::Path>>(cache_dir: P) -> Result<Self> {
        let cache = FileCache::new(cache_dir)?;
        Ok(Self {
            client: reqwest::Client::new(),
            cache: Some(cache),
            auth: None,
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

    /// Get breaking events - markets that moved the most in the last 24 hours
    ///
    /// This fetches markets ordered by price change and extracts unique events.
    /// Unlike the events endpoint, the markets endpoint supports ordering by
    /// oneDayPriceChange which is what Polymarket's "Breaking" page uses.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of events to return (default 50)
    pub async fn get_breaking_events(&self, limit: Option<usize>) -> Result<Vec<Event>> {
        let limit = limit.unwrap_or(50);
        // Fetch more markets than needed since we dedupe by event
        let market_limit = limit * 3;

        let url = format!(
            "{}/markets?active=true&closed=false&order=oneDayPriceChange&ascending=false&limit={}",
            GAMMA_API_BASE, market_limit
        );

        log_info!("GET {}", url);

        let response = self.client.get(&url).send().await?;
        let _status = response.status();

        log_info!("GET {} -> status: {}", url, _status);

        let markets: Vec<Market> = response.json().await?;

        // Extract unique events from markets, preserving order (biggest movers first)
        // Also track the max price change for each event
        let mut seen_event_ids = std::collections::HashSet::new();
        let mut event_price_changes: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        let mut events = Vec::new();

        for market in &markets {
            if let Some(event_ref) = market.event() {
                // Track the max absolute price change for this event
                if let Some(price_change) = market.one_day_price_change {
                    let abs_change = price_change.abs();
                    event_price_changes
                        .entry(event_ref.id.clone())
                        .and_modify(|e| *e = e.max(abs_change))
                        .or_insert(abs_change);
                }
            }
        }

        for market in markets {
            if let Some(event_ref) = market.event()
                && seen_event_ids.insert(event_ref.id.clone())
            {
                // Fetch full event data for this event
                if let Ok(Some(mut full_event)) = self.get_event_by_id(&event_ref.id).await {
                    // Set the max price change we tracked earlier
                    full_event.max_price_change_24hr =
                        event_price_changes.get(&event_ref.id).copied();
                    events.push(full_event);
                    if events.len() >= limit {
                        break;
                    }
                }
            }
        }

        log_info!(
            "Fetched {} breaking events from {} markets",
            events.len(),
            market_limit
        );
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

        // The search endpoint doesn't return volume data, so we need to fetch
        // full event details for each result to get market volumes
        let mut full_events = Vec::with_capacity(search_response.events.len());
        for event in &search_response.events {
            match self.get_event_by_slug(&event.slug).await {
                Ok(Some(full_event)) => full_events.push(full_event),
                Ok(None) => {
                    // Event not found, use the search result as-is
                    log_debug!("Event not found by slug: {}", event.slug);
                    full_events.push(event.clone());
                },
                Err(_e) => {
                    // Failed to fetch, use the search result as-is
                    log_debug!("Failed to fetch event {}: {}", event.slug, _e);
                    full_events.push(event.clone());
                },
            }
        }

        log_info!(
            "Enriched {} search results with full event data",
            full_events.len()
        );

        Ok(full_events)
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

    /// Check API health status
    pub async fn get_status(&self) -> Result<StatusResponse> {
        let url = format!("{}/status", GAMMA_API_BASE);
        let status: StatusResponse = self.client.get(&url).send().await?.json().await?;
        Ok(status)
    }

    /// Get tag by ID
    pub async fn get_tag_by_id(&self, tag_id: &str) -> Result<Option<Tag>> {
        let url = format!("{}/tags/{}", GAMMA_API_BASE, tag_id);
        let response = self.client.get(&url).send().await?;

        if response.status() == 404 {
            return Ok(None);
        }

        let tag: Tag = response.json().await?;
        Ok(Some(tag))
    }

    /// Get tag by slug
    pub async fn get_tag_by_slug(&self, slug: &str) -> Result<Option<Tag>> {
        let url = format!("{}/tags/slug/{}", GAMMA_API_BASE, slug);
        let response = self.client.get(&url).send().await?;

        if response.status() == 404 {
            return Ok(None);
        }

        let tag: Tag = response.json().await?;
        Ok(Some(tag))
    }

    /// Get related tags for a tag ID
    pub async fn get_related_tags(&self, tag_id: &str) -> Result<Vec<Tag>> {
        let url = format!("{}/tags/{}/related-tags", GAMMA_API_BASE, tag_id);
        let tags: Vec<Tag> = self.client.get(&url).send().await?.json().await?;
        Ok(tags)
    }

    /// Get all series
    pub async fn get_series(&self, limit: Option<usize>) -> Result<Vec<Series>> {
        let limit = limit.unwrap_or(100);
        let url = format!("{}/series?limit={}", GAMMA_API_BASE, limit);
        let series: Vec<Series> = self.client.get(&url).send().await?.json().await?;
        Ok(series)
    }

    /// Get series by ID
    pub async fn get_series_by_id(&self, series_id: &str) -> Result<Option<Series>> {
        let url = format!("{}/series/{}", GAMMA_API_BASE, series_id);
        let response = self.client.get(&url).send().await?;

        if response.status() == 404 {
            return Ok(None);
        }

        let series: Series = response.json().await?;
        Ok(Some(series))
    }

    /// Get public profile by wallet address
    pub async fn get_public_profile(&self, address: &str) -> Result<Option<PublicProfile>> {
        let url = format!("{}/public-profile", GAMMA_API_BASE);
        let params = [("address", address)];
        let response = self.client.get(&url).query(&params).send().await?;

        if response.status() == 404 {
            return Ok(None);
        }

        let profile: PublicProfile = response.json().await?;
        Ok(Some(profile))
    }

    /// Get tags for a specific event
    pub async fn get_event_tags(&self, event_id: &str) -> Result<Vec<Tag>> {
        let url = format!("{}/events/{}/tags", GAMMA_API_BASE, event_id);
        let tags: Vec<Tag> = self.client.get(&url).send().await?.json().await?;
        Ok(tags)
    }

    /// Get tags for a specific market
    pub async fn get_market_tags(&self, market_id: &str) -> Result<Vec<Tag>> {
        let url = format!("{}/markets/{}/tags", GAMMA_API_BASE, market_id);
        let tags: Vec<Tag> = self.client.get(&url).send().await?.json().await?;
        Ok(tags)
    }

    /// Create L2 authentication headers for a request
    /// This uses the same HMAC-based authentication as the CLOB API
    #[allow(dead_code)]
    fn create_auth_headers(
        &self,
        method: &str,
        request_path: &str,
        body: Option<&str>,
    ) -> Result<reqwest::header::HeaderMap> {
        use crate::clob::L2Headers;

        let auth = self.auth.as_ref().ok_or_else(|| {
            crate::error::PolymarketError::InvalidData("No auth credentials configured".to_string())
        })?;

        // Use the same L2Headers implementation as CLOB API
        let l2_headers = L2Headers::new(
            &auth.api_key,
            &auth.api_secret,
            &auth.passphrase,
            &auth.address,
            method,
            request_path,
            body,
        )
        .map_err(crate::error::PolymarketError::InvalidData)?;

        Ok(l2_headers.to_header_map())
    }

    /// Create cookie-based authentication headers for Gamma API
    /// This is required for endpoints like favorite_events that use browser session auth
    fn create_cookie_headers(&self) -> Result<reqwest::header::HeaderMap> {
        use reqwest::header::{COOKIE, HeaderMap, HeaderValue};

        let auth = self.auth.as_ref().ok_or_else(|| {
            crate::error::PolymarketError::InvalidData("No auth credentials configured".to_string())
        })?;

        let session_cookie = auth.session_cookie.as_ref().ok_or_else(|| {
            crate::error::PolymarketError::InvalidData(
                "Session cookies required for this endpoint. \
                 Get them from browser dev tools after logging in to polymarket.com"
                    .to_string(),
            )
        })?;

        // Build cookie string with all required cookies
        let mut cookies = vec![format!("polymarketsession={}", session_cookie)];

        if let Some(ref nonce) = auth.session_nonce {
            cookies.push(format!("polymarketnonce={}", nonce));
        }

        if let Some(ref auth_type) = auth.session_auth_type {
            cookies.push(format!("polymarketauthtype={}", auth_type));
        }

        let cookie_value = cookies.join("; ");

        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_str(&cookie_value).map_err(|e| {
                crate::error::PolymarketError::InvalidData(format!("Invalid cookie value: {}", e))
            })?,
        );

        Ok(headers)
    }

    /// Get all favorite events for the authenticated user
    /// Requires a valid session cookie (browser-based authentication)
    pub async fn get_favorite_events(&self) -> Result<Vec<FavoriteEvent>> {
        if !self.has_session_cookie() {
            return Err(crate::error::PolymarketError::InvalidData(
                "Session cookie required for favorite events. \
                 Add 'session_cookie' to your auth config with the value of \
                 'polymarketsession' cookie from browser dev tools."
                    .to_string(),
            ));
        }

        let url = format!("{}/favorite_events", GAMMA_API_BASE);
        let request_path = "/favorite_events";

        log_info!("GET {} (cookie auth)", url);

        let headers = self.create_cookie_headers()?;

        let response = self.client.get(&url).headers(headers).send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            log_warn!("GET {} -> error: {} - {}", request_path, status, body);
            return Err(crate::error::PolymarketError::InvalidData(format!(
                "Failed to get favorite events: {} - {}",
                status, body
            )));
        }

        log_info!("GET {} -> {}", request_path, status);
        let favorites: Vec<FavoriteEvent> = response.json().await?;
        log_info!("Fetched {} favorite events", favorites.len());
        Ok(favorites)
    }

    /// Add an event to favorites
    /// Requires a valid session cookie (browser-based authentication)
    pub async fn add_favorite_event(&self, event_id: &str) -> Result<FavoriteEvent> {
        if !self.has_session_cookie() {
            return Err(crate::error::PolymarketError::InvalidData(
                "Session cookie required for favorite events".to_string(),
            ));
        }

        let url = format!("{}/favorite_events", GAMMA_API_BASE);
        let request_path = "/favorite_events";

        let body = serde_json::to_string(&AddFavoriteRequest {
            event_id: event_id.to_string(),
        })?;

        log_info!("POST {} (cookie auth)", request_path);

        let headers = self.create_cookie_headers()?;

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            log_warn!("Failed to add favorite event: {} - {}", status, body);
            return Err(crate::error::PolymarketError::InvalidData(format!(
                "Failed to add favorite event: {}",
                status
            )));
        }

        let favorite: FavoriteEvent = response.json().await?;
        log_info!("Added favorite event: {}", event_id);
        Ok(favorite)
    }

    /// Remove an event from favorites
    /// Requires a valid session cookie (browser-based authentication)
    pub async fn remove_favorite_event(&self, favorite_id: i64) -> Result<()> {
        if !self.has_session_cookie() {
            return Err(crate::error::PolymarketError::InvalidData(
                "Session cookie required for favorite events".to_string(),
            ));
        }

        let url = format!("{}/favorite_events/{}", GAMMA_API_BASE, favorite_id);
        let request_path = format!("/favorite_events/{}", favorite_id);

        log_info!("DELETE {} (cookie auth)", request_path);

        let headers = self.create_cookie_headers()?;

        let response = self.client.delete(&url).headers(headers).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            log_warn!("Failed to remove favorite event: {} - {}", status, body);
            return Err(crate::error::PolymarketError::InvalidData(format!(
                "Failed to remove favorite event: {}",
                status
            )));
        }

        log_info!("Removed favorite event: {}", favorite_id);
        Ok(())
    }

    /// Check if an event is in favorites (returns the favorite entry if found)
    pub async fn is_favorite_event(&self, event_id: &str) -> Result<Option<FavoriteEvent>> {
        let favorites = self.get_favorite_events().await?;
        Ok(favorites.into_iter().find(|f| f.event_id == event_id))
    }

    /// Toggle favorite status for an event
    /// Returns true if the event is now a favorite, false if it was removed
    pub async fn toggle_favorite_event(&self, event_id: &str) -> Result<bool> {
        if let Some(favorite) = self.is_favorite_event(event_id).await? {
            self.remove_favorite_event(favorite.id).await?;
            Ok(false)
        } else {
            self.add_favorite_event(event_id).await?;
            Ok(true)
        }
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

/// API status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: String,
}

/// Series information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Public profile information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicProfile {
    #[serde(default)]
    pub address: Option<String>,
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

/// Favorite event entry from the Gamma API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteEvent {
    /// Favorite entry ID (used for deletion) - returned as string from API
    #[serde(deserialize_with = "deserialize_string_to_i64")]
    pub id: i64,
    /// Event ID
    pub event_id: String,
    /// User ID
    #[serde(default)]
    pub user_id: Option<String>,
    /// The full event data (optional, included when fetching favorites)
    #[serde(default)]
    pub event: Option<Event>,
    /// Creation timestamp
    #[serde(default)]
    pub created_at: Option<String>,
    /// Update timestamp
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Helper to deserialize string numbers to i64
fn deserialize_string_to_i64<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    s.parse::<i64>()
        .map_err(|e| D::Error::custom(format!("Failed to parse i64: {}", e)))
}

/// Request body for adding a favorite event
#[derive(Debug, Clone, Serialize)]
struct AddFavoriteRequest {
    event_id: String,
}

impl Default for GammaClient {
    fn default() -> Self {
        Self::new()
    }
}
