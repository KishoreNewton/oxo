//! # oxo-wasm
//!
//! WASM plugin system for the **oxo** observability TUI.
//!
//! Plugins are compiled to WebAssembly and loaded at runtime. They can:
//!
//! - **Transform** log entries (custom parsers, enrichers, redactors)
//! - **Filter** log entries (custom filter logic beyond pipeline stages)
//! - **Aggregate** log entries (custom analytics)
//!
//! ## Plugin API
//!
//! Plugins export functions that receive JSON-serialized log entries and
//! return transformed entries. The host provides imported functions for
//! logging and configuration access.
//!
//! ## Example plugin (Rust → WASM)
//!
//! ```ignore
//! #[no_mangle]
//! pub extern "C" fn transform(ptr: *const u8, len: usize) -> u64 {
//!     // Read input JSON, transform, return output JSON
//! }
//! ```

pub mod host;
pub mod plugin;
pub mod registry;

pub use plugin::{Plugin, PluginKind, PluginManifest};
pub use registry::PluginRegistry;
