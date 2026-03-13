//! # oxo-tui
//!
//! Terminal user interface for the **oxo** observability TUI.
//!
//! This crate contains all rendering, input handling, and UI state
//! management. It depends on [`oxo_core`] for the backend trait and types,
//! but never imports a specific backend directly.
//!
//! ## Architecture
//!
//! - [`app::App`] — Top-level state machine and async event loop
//! - [`components`] — Self-contained UI components (log viewer, query bar, etc.)
//! - [`layout`] — Panel arrangement and focus management
//! - [`event`] — Terminal event stream bridging
//! - [`theme`] — Color palette and styling
//! - [`keymap`] — Key binding definitions
//! - [`terminal`] — Terminal setup and teardown
//!
//! ## Usage
//!
//! ```no_run
//! use oxo_core::config::DisplayConfig;
//! use oxo_tui::app::App;
//!
//! # async fn example(backend: Box<dyn oxo_core::LogBackend>) {
//! let config = DisplayConfig::default();
//! let mut app = App::new(backend, config, Some("{job=\"api\"}".into()));
//! app.run().await.expect("app crashed");
//! # }
//! ```

pub mod action;
pub mod app;
pub mod components;
pub mod event;
pub mod keymap;
pub mod layout;
pub mod terminal;
pub mod theme;
