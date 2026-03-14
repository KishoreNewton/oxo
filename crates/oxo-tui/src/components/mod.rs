//! UI components for the oxo TUI.
//!
//! Each component is a self-contained unit that can handle input, update its
//! state, and render itself into a [`ratatui::Frame`] area.
//!
//! ## Layout
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │  QueryBar                                    │
//! ├──────────────┬──────────────────────────────┤
//! │ FilterPanel  │  LogViewer          Detail    │
//! │              │                     Panel     │
//! │              ├──────────────────────────────┤
//! │              │  Histogram / Sparkline         │
//! ├──────────────┴──────────────────────────────┤
//! │  StatusBar / SearchBar                       │
//! └─────────────────────────────────────────────┘
//! ```

pub mod alert_panel;
pub mod analytics_panel;
pub mod autocomplete;
pub mod detail_panel;
pub mod diff_view;
pub mod filter_panel;
pub mod health_dashboard;
pub mod help;
pub mod histogram;
pub mod incident_timeline;
pub mod live_dashboard;
pub mod log_viewer;
pub mod nl_query;
pub mod query_bar;
pub mod regex_playground;
pub mod saved_views;
pub mod search_bar;
pub mod source_picker;
pub mod sparkline;
pub mod stats_panel;
pub mod status_bar;
pub mod tab_bar;
pub mod time_picker;
pub mod trace_waterfall;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::action::Action;

/// Trait implemented by all TUI components.
pub trait Component {
    /// Handle a keyboard event (called only when focused).
    fn handle_key(&mut self, _key: KeyEvent) -> Option<Action> {
        None
    }

    /// Handle an action dispatched by the main loop.
    fn handle_action(&mut self, _action: &Action) -> Option<Action> {
        None
    }

    /// Render this component into the given frame area.
    fn render(&self, frame: &mut Frame, area: Rect, focused: bool);
}
