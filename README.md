# polymarket-tui

[![CI](https://github.com/penso/polymarket-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/penso/polymarket-tui/actions/workflows/ci.yml)

Real-time Polymarket monitoring tool built in Rust. Monitor live market
updates, trades, orderbook changes, and prices with beautiful colored console
output.

This was started as a pet project for me to play with AI. This used Cursor,
Claude and Codex.

## Features

- 🔴 **Real-time WebSocket monitoring** - Connect to Polymarket's WebSocket API for live updates
- 📊 **Market data discovery** - Automatically fetch active markets from Gamma API
- 🎨 **Colored console output** - Beautiful, color-coded display of trades, orderbooks, and prices
- 📈 **Multiple update types** - Monitor trades, orderbook changes, orders, and price updates
- 🚀 **Zero configuration** - No API keys required for public market data

## Workspace

- `crates/polymarket-api`: Core library with WebSocket client, Gamma API client, and display formatters
- `crates/cli`: Binary application for real-time monitoring

## Usage

### Run the CLI

```bash
cargo run -p polymarket-cli
```

The CLI will:

1. Fetch all active markets from Polymarket
2. Connect to the WebSocket stream
3. Display real-time updates with colored output

### Use the Library

```rust
use polymarket_api::{GammaClient, PolymarketWebSocket, MarketUpdateFormatter};

// Fetch active markets
let gamma = GammaClient::new();
let asset_ids = gamma.get_all_active_asset_ids().await?;

// Connect to WebSocket
let mut ws = PolymarketWebSocket::new(asset_ids);
ws.connect_and_listen(|msg| {
    let formatted = MarketUpdateFormatter::format_message(&msg, None);
    println!("{}", formatted);
}).await?;
```

## Development

- Build: `cargo build`
- Run CLI: `cargo run -p polymarket-cli`
- Test: `cargo test`
- Check: `cargo check`

## API Documentation

The library provides three main modules:

- **`gamma`**: Client for Polymarket's Gamma API (market discovery)
- **`websocket`**: WebSocket client for real-time market updates
- **`display`**: Formatters for colored console output

## CI

GitHub Actions runs on pushes and PRs:

- rustfmt check
- clippy with warnings denied
- full test suite
