use polymarket_bot::gamma::{GammaClient, Market};

#[test]
fn test_market_deserialization_with_json_string() {
    // Test that we can deserialize a market where clobTokenIds is a JSON string
    let json = r#"
    {
        "id": "123",
        "question": "Test market?",
        "clobTokenIds": "[\"token1\", \"token2\"]",
        "outcomes": "[\"Yes\", \"No\"]",
        "outcomePrices": "[\"0.5\", \"0.5\"]"
    }
    "#;

    let market: Market = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(market.id, "123");
    assert_eq!(market.question, "Test market?");
    assert_eq!(market.clob_token_ids, Some(vec!["token1".to_string(), "token2".to_string()]));
    assert_eq!(market.outcomes, "[\"Yes\", \"No\"]");
    assert_eq!(market.outcome_prices, "[\"0.5\", \"0.5\"]");
}

#[test]
fn test_market_deserialization_with_array() {
    // Test that we can deserialize a market where clobTokenIds is an array
    let json = r#"
    {
        "id": "456",
        "question": "Another market?",
        "clobTokenIds": ["token3", "token4"],
        "outcomes": "[\"Yes\", \"No\"]",
        "outcomePrices": "[\"0.6\", \"0.4\"]"
    }
    "#;

    let market: Market = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(market.id, "456");
    assert_eq!(market.clob_token_ids, Some(vec!["token3".to_string(), "token4".to_string()]));
}

#[test]
fn test_market_deserialization_without_clob_token_ids() {
    // Test that we can deserialize a market without clobTokenIds (optional field)
    let json = r#"
    {
        "id": "789",
        "question": "Market without tokens?",
        "outcomes": "[\"Yes\", \"No\"]",
        "outcomePrices": "[\"0.7\", \"0.3\"]"
    }
    "#;

    let market: Market = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(market.id, "789");
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
        "outcomes": "[\"Yes\", \"No\"]",
        "outcomePrices": "[\"0.8\", \"0.2\"]"
    }
    "#;

    let market: Market = serde_json::from_str(json).expect("Should deserialize");
    assert_eq!(market.id, "999");
    assert_eq!(market.clob_token_ids, None);
}

#[tokio::test]
async fn test_gamma_client_creation() {
    let _client = GammaClient::new();
    // Just verify it can be created (doesn't panic)
}

