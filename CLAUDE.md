# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
# Build (with TUI feature enabled)
just build              # or: cargo build --features tui
just build-release      # Release build

# Run CLI
cargo run -p polymarket-cli --features tui

# Quality checks (run all before committing)
just format-check       # Check formatting
just lint               # Clippy with -D warnings
cargo test              # Run tests

# Format code
just format
```

## Architecture

This is a Rust workspace with two crates:

- **`crates/polymarket-api`**: Core library providing:
  - `GammaClient` - Polymarket's Gamma API for market discovery
  - `ClobClient` - CLOB API for orderbook and trades
  - `DataClient` - Data API for trade history
  - `RTDSClient` - Real-time WebSocket for live trade activity
  - `PolymarketWebSocket` - WebSocket for orderbook/price updates
  - `MarketUpdateFormatter`/`RTDSFormatter` - Colored console output

- **`crates/cli`**: Binary application with subcommands:
  - `monitor` - Watch all active markets via WebSocket
  - `watch-event` - Watch specific event trades (supports `--tui` flag)
  - `trending` - Browse trending events in TUI
  - `orderbook`, `trades`, `event`, `market` - Query commands

## Key Patterns

- TUI features are behind `--features tui` flag (uses ratatui/crossterm)
- The `tracing` feature on polymarket-api enables logging integration
- File caching for market info uses `POLYMARKET_CACHE_DIR` env var or `~/.cache/polymarket-api/`

## Git Workflow

Follow conventional commit format: `feat|fix|refactor|docs|test|chore(scope): description`

Run all checks before committing:
1. `just format-check`
2. `just lint`
3. `cargo test`
