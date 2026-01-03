//! TUI for browsing trending events with live trade monitoring

mod event_loop;
mod fetch;
mod keys;
mod layout;
#[macro_use]
mod logging;
mod render;
pub mod state;

pub use {event_loop::run_trending_tui, state::TrendingAppState};
