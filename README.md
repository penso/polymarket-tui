# polymarket-bot

[![CI](https://github.com/OWNER/REPO/actions/workflows/ci.yml/badge.svg)](https://github.com/OWNER/REPO/actions/workflows/ci.yml)

Basic Rust workspace with a library crate and CLI.

Replace `OWNER/REPO` in the badge URL above after pushing to GitHub.

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

