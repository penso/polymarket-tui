# polymarket-tui

[![Crates.io](https://img.shields.io/crates/v/polymarket-tui.svg)](https://crates.io/crates/polymarket-tui)
[![Documentation](https://docs.rs/polymarket-tui/badge.svg)](https://docs.rs/polymarket-tui)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A terminal UI for browsing Polymarket prediction markets and watching live trades.

## Features

- **Browse trending events** - View markets sorted by volume, breaking news, or newest
- **Live trade monitoring** - Watch real-time trades via WebSocket
- **Market details** - See prices, volumes, and outcomes for each market
- **Keyboard navigation** - Full keyboard support with vim-style bindings
- **Mouse support** - Click to select events, scroll panels, switch tabs

## Installation

```bash
cargo install polymarket-tui
```

## Usage

```bash
# Browse trending events
polymarket-tui trending

# View help
polymarket-tui --help
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑`/`k`, `↓`/`j` | Navigate up/down |
| `Tab` | Switch between panels |
| `←`/`→` | Switch tabs (Trending/Breaking/New) |
| `Enter` | Toggle live trade watching |
| `/` | Search markets (API) |
| `f` | Filter current list (local) |
| `?` | Show help |
| `q` | Quit |

## Screenshot

The TUI displays:
- **Left panel**: Event list with volume and market count
- **Right panel**: Event details, markets with prices, and live trades

## Related

- [polymarket-api](https://crates.io/crates/polymarket-api) - The underlying API library

## License

MIT
