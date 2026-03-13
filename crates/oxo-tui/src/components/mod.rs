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
//! │              │  Sparkline                    │
//! ├──────────────┴──────────────────────────────┤
//! │  StatusBar / SearchBar                       │
//! └─────────────────────────────────────────────┘
//! ```

pub mod detail_panel;
pub mod filter_panel;
pub mod help;
pub mod log_viewer;
pub mod query_bar;
pub mod search_bar;
pub mod sparkline;
pub mod status_bar;

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
