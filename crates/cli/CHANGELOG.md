# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2](https://github.com/penso/polymarket-tui/compare/polymarket-tui-v0.1.1...polymarket-tui-v0.1.2) - 2026-01-03

### Added

- *(tui)* add yield opportunity icons to Trending tab
- *(tui)* improve yield display, fix emoji crash, add screenshot
- *(tui)* unify tabs, add rounded borders, gitui-style layout
- *(tui)* add colored tab backgrounds using tailwind palette
- *(tui)* add API search to Yield tab with yield opportunity display
- add logs panel toggle and improve UI layout
- add tab caching and improve UI layout

### Fixed

- *(tui)* correct mouse panel detection for Yield tab layout
- *(tui)* correct yield sort tab click detection ranges
- show fallback when user column is empty in trades
- give more space to Market column in trades table
- widen Market column in trades table
- mouse click selection now matches rendered positions

### Other

- apply nightly rustfmt formatting
- apply rustfmt formatting
- shrink Event Details panel by 2 lines
- improve Event Details panel layout

## [0.1.1](https://github.com/penso/polymarket-tui/compare/polymarket-tui-v0.1.0...polymarket-tui-v0.1.1) - 2026-01-01

### Added

- enable tracing feature by default

### Fixed

- enable tui feature by default

### Other

- add crate-specific README for polymarket-tui
