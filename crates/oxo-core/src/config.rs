//! Configuration types for oxo.
//!
//! These structures are deserialized from the user's config file
//! (`~/.config/oxo/config.toml`) and can be overridden by CLI flags.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Which backend to use (e.g. "loki", "elasticsearch").
    pub backend: String,

    /// Backend-specific connection settings.
    pub connection: ConnectionConfig,

    /// TUI display settings.
    pub display: DisplayConfig,

    /// Key binding overrides.
    pub keybindings: HashMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            backend: "demo".to_string(),
            connection: ConnectionConfig::default(),
            display: DisplayConfig::default(),
            keybindings: HashMap::new(),
        }
    }
}

/// Connection settings for the active backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionConfig {
    /// Backend URL (e.g. "http://localhost:3100" for Loki).
    pub url: String,

    /// Authentication configuration.
    pub auth: Option<AuthConfig>,

    /// Extra backend-specific key-value settings.
    ///
    /// These are passed through to the backend implementation as-is.
    /// For example, Loki uses `org_id` for multi-tenant setups.
    pub extra: HashMap<String, String>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:3100".to_string(),
            auth: None,
            extra: HashMap::new(),
        }
    }
}

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuthConfig {
    /// HTTP Basic authentication.
    #[serde(rename = "basic")]
    Basic { username: String, password: String },
    /// Bearer token authentication.
    #[serde(rename = "bearer")]
    Bearer { token: String },
}

/// TUI display settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Maximum number of log lines to keep in the buffer.
    pub max_buffer_size: usize,

    /// Whether to enable mouse support.
    pub mouse: bool,

    /// Tick rate in milliseconds for sparkline updates.
    pub tick_rate_ms: u64,

    /// Render interval in milliseconds.
    pub render_rate_ms: u64,

    /// Whether to show timestamps in log lines.
    pub show_timestamps: bool,

    /// Whether to wrap long lines.
    pub line_wrap: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            max_buffer_size: 50_000,
            mouse: true,
            tick_rate_ms: 250,
            render_rate_ms: 50,
            show_timestamps: true,
            line_wrap: false,
        }
    }
}
