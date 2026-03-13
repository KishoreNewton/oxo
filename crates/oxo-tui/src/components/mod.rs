//! UI components for the oxo TUI.
//!
//! Each component is a self-contained unit that can handle input, update its
//! state, and render itself into a [`ratatui::Frame`] area. Components
//! communicate with the rest of the application by returning [`Action`]s.
//!
//! ## Component trait
//!
//! All components implement the [`Component`] trait, which defines three
//! lifecycle methods:
//!
//! 1. [`handle_key`](Component::handle_key) — process keyboard input
//! 2. [`handle_action`](Component::handle_action) — react to dispatched actions
//! 3. [`render`](Component::render) — draw the component into a frame area
//!
//! ## Layout
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │  QueryBar                                    │
//! ├──────────────┬──────────────────────────────┤
//! │ FilterPanel  │  LogViewer                    │
//! │              │                               │
//! │              │                               │
//! │              ├──────────────────────────────┤
//! │              │  Sparkline                    │
//! ├──────────────┴──────────────────────────────┤
//! │  StatusBar                                   │
//! └─────────────────────────────────────────────┘
//! ```

pub mod filter_panel;
pub mod help;
pub mod log_viewer;
pub mod query_bar;
pub mod sparkline;
pub mod status_bar;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::action::Action;

/// Trait implemented by all TUI components.
///
/// Components are the building blocks of the UI. They own their local state,
/// respond to input, and render themselves.
pub trait Component {
    /// Handle a keyboard event.
    ///
    /// Called only when this component has focus. Returns an [`Action`] if
    /// the key press should trigger a state change.
    fn handle_key(&mut self, _key: KeyEvent) -> Option<Action> {
        None
    }

    /// Handle an action dispatched by the main loop.
    ///
    /// Called for every action, regardless of focus. This allows components
    /// to react to global events (e.g. a log batch arriving).
    ///
    /// Returns an optional follow-up action.
    fn handle_action(&mut self, _action: &Action) -> Option<Action> {
        None
    }

    /// Render this component into the given frame area.
    ///
    /// # Arguments
    ///
    /// * `frame` — The ratatui frame to draw into.
    /// * `area` — The rectangular area allocated to this component.
    /// * `focused` — Whether this component currently has focus (affects border style).
    fn render(&self, frame: &mut Frame, area: Rect, focused: bool);
}
