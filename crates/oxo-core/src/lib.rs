//! # oxo-core
//!
//! Core types and traits for the **oxo** observability TUI.
//!
//! This crate defines the interface contract between backends (Loki,
//! Elasticsearch, CloudWatch, etc.) and the terminal UI. It contains:
//!
//! - [`backend::LogBackend`] — the trait every backend implements
//! - [`backend::LogEntry`] — the normalized log entry type
//! - [`error::BackendError`] — the unified error type
//! - [`event::BackendEvent`] — events flowing from backends to the UI
//! - [`query::TimeRange`] — time range for historical queries
//! - [`config`] — configuration structures
//!
//! Backend crates (e.g. `oxo-loki`) depend on this crate. The TUI crate
//! (`oxo-tui`) also depends on this crate but never on a specific backend.

pub mod backend;
pub mod config;
pub mod error;
pub mod event;
pub mod query;

// Re-export the most commonly used types at the crate root for convenience.
pub use backend::{LogBackend, LogEntry, TailHandle};
pub use config::AppConfig;
pub use error::BackendError;
pub use event::BackendEvent;
pub use query::TimeRange;
