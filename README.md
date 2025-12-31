# polymarket-bot

[![CI](https://github.com/penso/polymarket-bot/actions/workflows/ci.yml/badge.svg)](https://github.com/penso/polymarket-bot/actions/workflows/ci.yml)

Basic Rust workspace with a library crate and CLI.

Badge targets `penso/polymarket-bot` based on the configured origin.

## Workspace
- `crates/polymarket-bot`: library exposing `greet()`
- `crates/cli`: binary depending on the library

## Development
- Build: `cargo build`
- Run CLI: `cargo run -p polymarket-cli`
- Test: `cargo test`

## CI
GitHub Actions runs on pushes and PRs:
- rustfmt check
- clippy with warnings denied
- full test suite
