use polymarket_bot::greet;

#[test]
fn greet_returns_expected_message() {
    let msg = greet();
    assert!(msg.contains("Hello from polymarket-bot"));
}

