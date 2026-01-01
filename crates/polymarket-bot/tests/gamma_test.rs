use polymarket_bot::gamma::{GammaClient, Market};

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
    assert_eq!(market.clob_token_ids, Some(vec!["token1".to_string(), "token2".to_string()]));
    assert_eq!(market.outcomes, vec!["Yes".to_string(), "No".to_string()]);
    assert_eq!(market.outcome_prices, vec!["0.5".to_string(), "0.5".to_string()]);
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
    assert_eq!(market.clob_token_ids, Some(vec!["token3".to_string(), "token4".to_string()]));
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

