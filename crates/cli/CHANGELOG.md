# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3](https://github.com/penso/polymarket-tui/compare/polymarket-tui-v0.1.2...polymarket-tui-v0.1.3) - 2026-01-03

### Added

- *(tui)* redesign trade popup with clickable tabs and fixed width
- *(tui)* add lazygit-style position indicators and fix orderbook height for closed markets
- *(tui)* add clickable tabs in orderbook panel title
- *(tui)* add thousands separators to orderbook shares and total
- *(tui)* make orderbook panel height dynamic based on data
- *(tui)* add Shift+S to save logs to file
- *(tui)* add Buy Yes/No buttons to market lines
- *(tui)* add mouse click to select market in orderbook panel
- *(tui)* add keyboard navigation for market selection in orderbook
- *(tui)* add order book panel for market depth visualization
- *(tui)* check both Gamma and Data API status with logging
- *(tui)* add API status indicator dot next to login/profile button
- *(tui)* add context-aware help modal and fix favorites alignment
- *(tui)* show sort-relevant metric in event list
- *(tui)* add sorting options for Events tab
- *(tui)* hide yield search input on Enter while keeping results
- *(tui)* improve yield tab event details and remove New tab
- *(tui)* allow / and f shortcuts to work from any panel
- *(tui)* show favorite icon in yield list and search results
- *(tui)* enhance favorites and portfolio features
- *(favorites)* implement cookie-based auth for Gamma API favorites
- *(tui)* add Favorites tab with authenticated API support
- *(auth)* add login system with credential storage and CLOB API enhancements
- *(tui)* add login button with placeholder modal
- *(tui)* add yield opportunity icons to Trending tab
- *(tui)* improve yield display, fix emoji crash, add screenshot
- *(tui)* unify tabs, add rounded borders, gitui-style layout
- *(tui)* add colored tab backgrounds using tailwind palette
- *(tui)* add API search to Yield tab with yield opportunity display
- add logs panel toggle and improve UI layout
- add tab caching and improve UI layout
- enable tracing feature by default
- improve TUI with ratatui 0.29 features
- hide websocket trade counts from events list
- add double-click to toggle trade websocket
- increase event details height and focus panels on hover
- add loading indicators for search and infinite scroll
- add mouse support and improve TUI event display
- add 'r' shortcut to refresh events list in TUI
- show expired marker on events in trending list
- format event total volume in short form
- show volume for each market in TUI markets panel
- show in-review status in TUI markets panel
- add in-review status for markets in UMA resolution
- add colored output for status fields in yield command
- add status display for events and markets in yield command
- add event end dates and --expires-in filter to yield command
- add market URLs to yield command output
- add yield command to find high-probability markets
- use short market names in trades panel
- use short market names (groupItemTitle) when available
- improve market status display with better icons and winner display
- show market status with emoji and sort by resolution status
- show market status and skip price fetching for resolved markets
- format market prices as cents like Polymarket website
- add tracing feature flag for optional logging
- *(tui)* add API-driven markets panel to both TUIs
- fetch market prices from CLOB API instead of local trades
- add current price and percentage to markets panel
- add scrolling and scrollbar to event details panel
- add filter options (Trending/Breaking/New) to header panel
- add trades count to trades panel title
- add Logs panel to Tab focus cycle
- show focused panel in footer
- add scrollbars and improve panel focus indicators
- add Tab navigation and scrolling between panels
- add end time, total volume, and image URL to event details
- update UI text to show local filter mode
- add local filter mode with 'f' key shortcut
- add infinite scrolling for trending events
- implement automatic scrolling for Logs panel
- add task monitoring to verify search task execution
- implement API-based search for events in TUI
- capture tracing logs and display in TUI log area
- add dedicated search input field to display current search value
- add search functionality to trending TUI
- show event details above trades in trending TUI
- enhance trending TUI with live trade monitoring
- add trending events TUI browser
- add CLOB REST API, Data API, and additional Gamma API endpoints
- add ratatui TUI for trade monitoring
- automatically use authentication from environment variables
- add CLI flags for RTDS and implement PING/PONG
- add RTDS WebSocket support for real-time trade activity
- add generic file-based caching for market info
- add tracing support and improve error handling

### Fixed

- *(tui)* handle mouse clicks on favorites panel events list
- *(tui)* show 'Market is closed' for closed markets in orderbook panel
- *(tui)* skip orderbook fetch for inactive/closed markets
- *(tui)* preserve orderbook panel height during loading
- *(tui)* size orderbook panel based on actual data
- *(tui)* fetch orderbook for initially selected market on startup
- *(tui)* shorten orderbook title to fit narrow depth chart panel
- *(tui)* use 1 decimal place for orderbook spread
- *(tui)* use 1 decimal place for orderbook prices
- *(tui)* keep orderbook panel height stable during loading
- *(tui)* make buttons truly adjacent with no space between
- *(tui)* right-align buttons and remove space between them
- *(tui)* make market line layout more compact
- *(tui)* use orderbook best ask price for market buttons
- *(tui)* use 1 decimal place for high prices to match website
- *(tui)* show actual sub-cent prices instead of '<1¢'
- *(tui)* correct thousands separator placement
- *(tui)* synchronize depth chart and price level row counts
- *(tui)* align depth chart bars with price data rows
- *(tui)* scale depth chart bars independently for bids and asks
- *(tui)* sort orderbook bids/asks by price before display
- *(tui)* use sorted markets list for orderbook selection
- *(tui)* correct orderbook TOTAL column and depth chart
- *(tui)* align market list columns with fixed widths
- *(tui)* enable mouse clicks on Markets panel in Favorites tab
- *(tui)* use actual outcome names in buttons and fix orderbook title
- *(tui)* improve orderbook behavior and add to favorites tab
- *(tui)* fix orderbook title truncation and reset on tab switch
- *(tui)* right-align orderbook price/shares/total columns
- *(tui)* improve orderbook depth chart and price display
- *(tui)* fix orderbook title and handle empty orderbook case
- *(tui)* show actual outcome names in orderbook instead of Yes/No
- *(tui)* update yield details with accurate profit calculation
- *(tui)* allow 'p' key to close profile modal
- *(tui)* toggle profile modal with 'p' key and fix modal width
- *(tui)* use fixed width for profile modal matching separator lines
- *(tui)* make profile modal narrower (50% instead of 70%)
- *(tui)* add Clear widget to help modal for solid background
- *(tui)* remove dim overlay, keep content visible behind modals
- *(tui)* lighter dim overlay and consistent panel titles
- *(tui)* fix help modal background transparency
- *(tui)* clear search results and query when exiting search mode
- *(tui)* keep yield search results visible after hiding search input
- *(tui)* show loading indicator in panel title instead of content
- *(tui)* maintain 2-panel layout in yield tab when event is loading
- *(tui)* fetch missing events for yield tab details panel
- *(tui)* calculate dynamic height for yield event details panel
- *(tui)* fix favorites tab showing breaking loading message
- *(tui)* allow / and f to add to search query when already in search mode
- *(tui)* fix bookmark toggle in yield tab by fetching event if not cached
- *(tui)* correct mouse panel detection for Yield tab layout
- *(tui)* correct yield sort tab click detection ranges
- show fallback when user column is empty in trades
- give more space to Market column in trades table
- widen Market column in trades table
- mouse click selection now matches rendered positions
- enable tui feature by default
- use tokio::spawn instead of Box::pin in double-click handler
- use correct API order values for event filters
- restrict 'o' key to EventDetails panel only
- start TUI with events list focused instead of header
- use 24hr volume for event total volume calculation
- reset markets panel scroll when switching events
- use correct event URLs in yield command
- use 24h volume instead of total volume in yield command
- update test for renamed binary and add CI deps to publish
- resolve fmt and clippy warnings
- handle empty groupItemTitle in market display
- use unicode display width for proper text alignment
- use minimum ask price instead of first ask
- remove emoji circles from SIDE column in trades listing
- *(tui)* use Gamma API outcome_prices instead of CLOB orderbook
- make Enter key only active when EventsList panel is focused
- improve orderbook error handling and fix linting issues
- improve orderbook error handling and reduce log noise
- correct CLOB API base URL
- use viewport_content_length instead of content_length for scrollbars
- improve markets panel scrollbar calculation and movement
- ensure scrollbars have proportional thumb size
- change trending events panel to show trades / markets
- calculate market question truncation based on available width
- improve Down arrow scroll calculation for Logs panel
- prevent auto-scroll from overriding manual scroll in Logs panel
- add focus highlighting to Logs panel rendering
- complete Logs panel focus implementation
- add focus highlighting to empty trades panel state
- add focus highlighting to event details and trades panels
- complete scrollbar and footer updates
- always use filtered events for selected event
- remove duplicate title truncation logic
- properly show market count / trade count at end when watching
- show market count / trade count at end of line when watching
- correct trending events sort order
- use correct event slug from search results when watching
- prevent duplicate RTDS message warnings and wrap long log lines
- account for List widget borders in width calculation
- remove line numbers and right-align market counts in events list
- remove unused variable in events list rendering
- format trending events as single line and add RTDS message logging
- add logging for trade reception in RTDS WebSocket
- add logging for RTDS WebSocket connections in TUI
- handle optional Market.id in CLI output
- use init() instead of set_default() for global dispatcher
- ensure tracing subscriber guard stays alive
- create search span as child of current span for proper context inheritance
- enter span briefly to register with subscriber before spawning
- remove span guard to fix borrow checker error
- ensure search span is entered before spawning task
- simplify tracing context inheritance for search tasks
- remove verbose character press logs and fix warnings
- ensure tracing context is inherited by spawned search tasks
- simplify span usage - rely on .instrument() for context inheritance
- create span before spawning to ensure proper context inheritance
- capture current tracing span before spawning search task
- simplify span creation for search task
- properly create and enter span for search task tracing
- move TuiLogLayer setup before API calls to capture all logs
- improve tracing context inheritance for search tasks
- improve search debounce logic and ensure URL logging
- strip existing [LEVEL] prefix before calling add_log to prevent double prefix
- use LogVisitor and add defensive prefix stripping in template extraction
- revert to simpler approach and add comprehensive prefix stripping
- remove unused variables and fix compilation errors
- improve message cleaning with better prefix detection
- improve prefix removal logic with better loop handling
- add prefix stripping in LogVisitor to prevent double [INFO]
- improve prefix removal to handle nested [INFO] prefixes
- improve message extraction and add URL logging to trending events API
- improve message cleaning to remove quotes and whitespace
- move tracing setup before any log calls and improve message formatting
- clone query before using in tracing span to avoid move error
- use tracing::Instrument to ensure spawned task logs are captured
- properly format tracing messages and ensure spawned task logs are captured
- attempt to format tracing messages with field values
- improve tracing event field extraction for better log capture
- improve log message extraction and prefix stripping
- clone query before set_search_results to avoid move error
- clone query before moving into async task to fix compilation
- remove duplicate [INFO] prefix and improve log capture
- clone query string for logging to avoid move errors
- ensure all search API logs are captured and add error handling
- improve search logging and remove 2-char minimum
- use correct public-search endpoint for event search
- use new GammaClient instance instead of cloning
- increase search field height to ensure visibility
- display search query on same line with proper styling
- make search query text visible with bold cyan styling
- increase search box height to prevent truncation
- correct indentation and closing brace for Enter key handler
- complete search mode keyboard handling implementation
- update header and footer to show search mode status
- ensure event_trades entry exists before starting websocket
- resolve move semantics issue in websocket closure
- add missing TokioMutex import in run_trending function
- resolve all build warnings
- add Trending variant to Commands enum
- remove unnecessary mut terminal variable
- correct TokioMutex import in tui.rs and remove unused trait methods
- use TradeDisplay trait in run_monitor_rtds and remove RTDSFormatter usage
- export RTDSFormatter and remove unused import
- handle empty messages and PONG responses in RTDS
- remove CLOB auth from activity subscriptions

### Other

- *(tui)* extract TrendingAppState to state/app_state.rs
- *(tui)* reduce trending_tui/mod.rs to imports only
- *(tui)* reduce render/mod.rs to imports only
- *(tui)* extract events list rendering to render/events_list.rs
- *(tui)* extract event details rendering to render/event_details.rs
- *(tui)* extract trades rendering to render/trades.rs
- *(tui)* extract markets rendering to render/markets.rs
- *(tui)* extract orderbook rendering to render/orderbook.rs
- *(tui)* extract header rendering to render/header.rs
- *(tui)* extract favorites tab rendering to render/favorites.rs
- *(tui)* extract yield tab rendering to render/yield_tab.rs
- *(tui)* extract logs and popups rendering to separate modules
- *(tui)* extract utility functions to render/utils.rs
- *(tui)* move render.rs to render/ directory structure
- *(tui)* split state.rs into state/ subdirectory modules
- *(tui)* log first and last bid/ask to debug ordering
- *(tui)* add more orderbook logging to debug token selection
- *(tui)* add logging to orderbook toggle for debugging
- *(tui)* add global event cache and DRY event details rendering
- release
- apply nightly rustfmt formatting
- apply rustfmt formatting
- shrink Event Details panel by 2 lines
- improve Event Details panel layout
- *(polymarket-tui)* release v0.1.1
- add crate-specific README for polymarket-tui
- upgrade ratatui to 0.30
- apply formatter changes
- add 'r: Refresh' hint to events list footer
- add workflow to publish crates to crates.io
- prepare crates for crates.io publishing
- add MIT license
- use batch API for fetching market prices
- attach keys to panels with context-sensitive help
- rename polymarket-tui crate to polymarket-api
- upgrade to Rust edition 2024
- add rustfmt and clippy config with consistent formatting
- *(trending_tui)* split into separate files by concern
- *(tui)* add debug logging for market data refresh
- rename project from polymarket-bot to polymarket-tui
- remove image from event details and make height dynamic
- improve TrendingAppState structure with domain-specific structs
- remove unused visible_height variable in render_markets
- make event details compact and add markets panel
- remove .instrument() to see if default dispatcher captures logs
- simplify span usage - .instrument() should work
- add test log inside spawned task to verify tracing context
- remove task monitoring code that was adding noise to logs
- remove excessive debug logging and improve log readability
- add more eprintln statements to track execution flow
- add extensive logging to diagnose search task execution
- remove test_log_layer binary
- make ratatui optional behind feature flag with trait-based display
- update CLI messages to clarify activity subscriptions are public
- Add justfile with lint, format, and build recipes
- normalize trailing newlines to satisfy rustfmt --check
- first commit

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
