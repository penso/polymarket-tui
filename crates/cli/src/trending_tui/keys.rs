//! Key bindings organized by panel
//!
//! This module defines which keys are active for each panel, making the key
//! handling more modular and easier to maintain.
//!
//! ## Panel-specific keys:
//! - **Header**: ←/→ to switch filters
//! - **EventsList**: / for API search, f for local filter, r to refresh, Enter to watch/unwatch
//! - **Markets**: r to refresh prices
//! - **All panels**: ↑/↓ to scroll, Tab to switch panels, q to quit

use super::state::FocusedPanel;

impl FocusedPanel {
    /// Returns a short help string for display in the footer
    pub fn help_text(&self) -> &'static str {
        match self {
            FocusedPanel::Header => "←/→: Filter",
            FocusedPanel::EventsList => "/: Search | f: Filter | r: Refresh | Enter: Watch",
            FocusedPanel::EventDetails => "o: Open URL | ↑/↓: Scroll",
            FocusedPanel::Markets => "r: Refresh | ↑/↓: Scroll",
            FocusedPanel::Trades => "↑/↓: Scroll",
            FocusedPanel::Logs => "↑/↓: Scroll",
        }
    }

    /// Returns the panel name for display
    pub fn name(&self) -> &'static str {
        match self {
            FocusedPanel::Header => "Header",
            FocusedPanel::EventsList => "Events",
            FocusedPanel::EventDetails => "Details",
            FocusedPanel::Markets => "Markets",
            FocusedPanel::Trades => "Trades",
            FocusedPanel::Logs => "Logs",
        }
    }
}
