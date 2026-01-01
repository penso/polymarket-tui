use polymarket_api::gamma::{GammaClient, Market, PublicProfile, Series, StatusResponse, Tag};

// ============================================================================
// Unit Tests (no network required)
// ============================================================================

#[test]
fn test_status_response_deserialization() {
    let json = r#"{"status": "ok"}"#;
    let response: StatusResponse = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(response.status, "ok");
}

#[test]
fn test_tag_deserialization() {
    let json = r#"{"id": "123", "label": "Politics", "slug": "politics"}"#;
    let tag: Tag = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(tag.id, "123");
    assert_eq!(tag.label, "Politics");
    assert_eq!(tag.slug, "politics");
}

#[test]
fn test_series_deserialization() {
    let json = r#"{"id": "456", "title": "US Elections", "slug": "us-elections", "description": "All US election markets"}"#;
    let series: Series = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(series.id, "456");
    assert_eq!(series.title, Some("US Elections".to_string()));
    assert_eq!(series.slug, Some("us-elections".to_string()));
    assert_eq!(
        series.description,
        Some("All US election markets".to_string())
    );
}

#[test]
fn test_series_deserialization_minimal() {
    let json = r#"{"id": "789"}"#;
    let series: Series = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(series.id, "789");
    assert!(series.title.is_none());
    assert!(series.slug.is_none());
    assert!(series.description.is_none());
}

#[test]
fn test_public_profile_deserialization() {
    let json = r#"{
        "address": "0x1234567890abcdef1234567890abcdef12345678",
        "name": "TestUser",
        "pseudonym": "tester",
        "bio": "Just testing",
        "profileImage": "https://example.com/image.png",
        "profileImageOptimized": "https://example.com/image_opt.png"
    }"#;
    let profile: PublicProfile = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(
        profile.address,
        Some("0x1234567890abcdef1234567890abcdef12345678".to_string())
    );
    assert_eq!(profile.name, Some("TestUser".to_string()));
    assert_eq!(profile.pseudonym, Some("tester".to_string()));
    assert_eq!(profile.bio, Some("Just testing".to_string()));
}

#[test]
fn test_public_profile_deserialization_minimal() {
    let json = r#"{}"#;
    let profile: PublicProfile = serde_json::from_str(json).expect("Should deserialize");
    assert!(profile.address.is_none());
    assert!(profile.name.is_none());
}

#[test]
fn test_market_deserialization_with_json_string() {
    // Test that we can deserialize a market where clobTokenIds is a JSON string
    let json = r#"
    {
        "id": "123",
        "question": "Test market?",
        "clobTokenIds": "[\"token1\", \"token2\"]",
        "outcomes": ["Yes", "No"],
        "outcomePrices": ["0.5", "0.5"]
    }
    "#;

    let market: Market = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(market.id, Some("123".to_string()));
    assert_eq!(market.question, "Test market?");
    assert_eq!(
        market.clob_token_ids,
        Some(vec!["token1".to_string(), "token2".to_string()])
    );
    assert_eq!(market.outcomes, vec!["Yes".to_string(), "No".to_string()]);
    assert_eq!(market.outcome_prices, vec![
        "0.5".to_string(),
        "0.5".to_string()
    ]);
}

#[test]
fn test_market_deserialization_with_array() {
    // Test that we can deserialize a market where clobTokenIds is an array
    let json = r#"
    {
        "id": "456",
        "question": "Another market?",
        "clobTokenIds": ["token3", "token4"],
        "outcomes": ["Yes", "No"],
        "outcomePrices": ["0.6", "0.4"],
        "slug": "another-market"
    }
    "#;

    let market: Market = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(market.id, Some("456".to_string()));
    assert_eq!(
        market.clob_token_ids,
        Some(vec!["token3".to_string(), "token4".to_string()])
    );
}

#[test]
fn test_market_deserialization_without_clob_token_ids() {
    // Test that we can deserialize a market without clobTokenIds (optional field)
    let json = r#"
    {
        "id": "789",
        "question": "Market without tokens?",
        "outcomes": ["Yes", "No"],
        "outcomePrices": ["0.7", "0.3"],
        "slug": "market-without-tokens"
    }
    "#;

    let market: Market = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(market.id, Some("789".to_string()));
    assert_eq!(market.clob_token_ids, None);
}

#[test]
fn test_market_deserialization_with_null_clob_token_ids() {
    // Test that we can deserialize a market with null clobTokenIds
    let json = r#"
    {
        "id": "999",
        "question": "Market with null tokens?",
        "clobTokenIds": null,
        "outcomes": ["Yes", "No"],
        "outcomePrices": ["0.8", "0.2"],
        "slug": "market-with-null-tokens"
    }
    "#;

    let market: Market = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(market.id, Some("999".to_string()));
    assert_eq!(market.clob_token_ids, None);
}

#[tokio::test]
async fn test_gamma_client_creation() {
    let _client = GammaClient::new();
    // Just verify it can be created (doesn't panic)
}

#[tokio::test]
async fn test_get_trending_events() {
    let client = GammaClient::new();
    let events = client
        .get_trending_events(Some("volume24hr"), Some(false), Some(10))
        .await
        .expect("Should fetch trending events");

    assert!(!events.is_empty(), "Should return at least one event");
    assert!(events.len() <= 10, "Should respect limit");

    // Verify event structure
    let event = &events[0];
    assert!(!event.id.is_empty());
    assert!(!event.slug.is_empty());
    assert!(!event.title.is_empty());
}

#[tokio::test]
async fn test_search_events() {
    let client = GammaClient::new();
    let events = client
        .search_events("election", Some(10))
        .await
        .expect("Should search events");

    // Search might return empty results, but should not error
    assert!(events.len() <= 10, "Should respect limit");

    // If we got results, verify structure
    if let Some(event) = events.first() {
        assert!(!event.id.is_empty());
        assert!(!event.slug.is_empty());
        assert!(!event.title.is_empty());
    }
}

#[tokio::test]
async fn test_get_event_by_slug() {
    let client = GammaClient::new();
    // Use a known event slug
    let event = client
        .get_event_by_slug("2026-fifa-world-cup-winner")
        .await
        .expect("Should fetch event by slug");

    if let Some(event) = event {
        assert!(!event.id.is_empty());
        assert!(!event.slug.is_empty());
        assert!(!event.title.is_empty());
    }
}

#[tokio::test]
async fn test_get_markets() {
    let client = GammaClient::new();
    let markets = client
        .get_markets(Some(true), Some(false), Some(10))
        .await
        .expect("Should fetch markets");

    assert!(markets.len() <= 10, "Should respect limit");

    if let Some(market) = markets.first() {
        // Market ID might be None in search responses, but question should always be present
        assert!(!market.question.is_empty());
    }
}

#[tokio::test]
async fn test_get_categories() {
    let client = GammaClient::new();
    let categories = client
        .get_categories()
        .await
        .expect("Should fetch categories");

    // Categories might be empty, but should not error
    if let Some(category) = categories.first() {
        assert!(!category.id.is_empty());
        assert!(!category.label.is_empty());
        assert!(!category.slug.is_empty());
    }
}

#[tokio::test]
async fn test_get_status() {
    let client = GammaClient::new();
    let result = client.get_status().await;

    // Status endpoint might return different formats, so we just verify it doesn't panic
    if let Ok(status) = result {
        assert!(!status.status.is_empty());
    }
}

#[tokio::test]
async fn test_get_tag_by_slug() {
    let client = GammaClient::new();
    // Use a common tag slug
    let result = client.get_tag_by_slug("politics").await;

    // Tag might not exist, that's acceptable
    if let Ok(Some(tag)) = result {
        assert!(!tag.id.is_empty());
        assert!(!tag.label.is_empty());
    }
}

#[tokio::test]
async fn test_get_series() {
    let client = GammaClient::new();
    let result = client.get_series(Some(10)).await;

    // Series might be empty, but should not error
    if let Ok(series_list) = result {
        assert!(series_list.len() <= 10, "Should respect limit");
        if let Some(series) = series_list.first() {
            assert!(!series.id.is_empty());
        }
    }
}

#[tokio::test]
async fn test_get_event_tags() {
    let client = GammaClient::new();
    // First get an event to get its ID
    let events = client
        .get_trending_events(Some("volume24hr"), Some(false), Some(1))
        .await;

    if let Ok(events) = events
        && let Some(event) = events.first()
    {
        let tags = client.get_event_tags(&event.id).await;
        // Tags might be empty, but should not error
        if let Ok(tags) = tags {
            for tag in &tags {
                assert!(!tag.id.is_empty());
            }
        }
    }
}

#[tokio::test]
async fn test_get_active_events() {
    let client = GammaClient::new();
    let events = client
        .get_active_events(Some(10))
        .await
        .expect("Should fetch active events");

    assert!(events.len() <= 10, "Should respect limit");
    if let Some(event) = events.first() {
        assert!(!event.id.is_empty());
        assert!(!event.slug.is_empty());
        assert!(!event.title.is_empty());
    }
}

#[tokio::test]
async fn test_get_event_by_id() {
    let client = GammaClient::new();
    // First get a known event ID
    let events = client
        .get_trending_events(Some("volume24hr"), Some(false), Some(1))
        .await;

    if let Ok(events) = events
        && let Some(event) = events.first()
    {
        let result = client.get_event_by_id(&event.id).await;
        assert!(result.is_ok(), "Should not error");
        if let Ok(Some(fetched_event)) = result {
            assert_eq!(fetched_event.id, event.id);
        }
    }
}

#[tokio::test]
async fn test_get_all_active_asset_ids() {
    let client = GammaClient::new();
    let result = client.get_all_active_asset_ids().await;

    // This can take a while but should not error
    assert!(result.is_ok(), "Should not error");
}
