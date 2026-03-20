//! Configuration types for oxo.
//!
//! These structures are deserialized from the user's config file
//! (`~/.config/oxo/config.toml`) and can be overridden by CLI flags.
//!
//! ## Source configuration
//!
//! Sources are defined as a flat `[[sources]]` array — each entry is a single
//! block with no nesting required:
//!
//! ```toml
//! [[sources]]
//! name = "Production Loki"
//! url  = "https://loki.prod:3100"
//! token = "my-bearer-token"
//!
//! [[sources]]
//! name = "Local files"
//! type = "file"
//! path = "/var/log/myapp/*.log"
//!
//! [[sources]]
//! name = "Demo"
//! type = "demo"
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Source (flat, simple) ───────────────────────────────────────────

/// A named log source — kept intentionally flat so users never have to
/// write nested TOML tables.
///
/// # Smart defaults
///
/// * If `type` is omitted, it is auto-detected from the `url`:
///   - URL contains `:3100` or `loki` → `"loki"`
///   - No URL → `"demo"`
/// * `token` is shorthand for Bearer auth, `username`/`password` for Basic.
/// * `path` is used by the `file` backend to tail local log files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    /// Human-readable name shown in the source picker.
    pub name: String,

    /// Backend type: `"loki"`, `"demo"`, `"file"`, etc.
    /// Auto-detected from `url` when omitted.
    #[serde(rename = "type", default)]
    pub backend_type: String,

    /// Backend URL (e.g. `"http://localhost:3100"`).
    #[serde(default)]
    pub url: String,

    /// Bearer token (shorthand for Bearer auth).
    #[serde(default)]
    pub token: String,

    /// Username for Basic auth.
    #[serde(default)]
    pub username: String,

    /// Password for Basic auth.
    #[serde(default)]
    pub password: String,

    /// Tenant / org ID (for multi-tenant Loki).
    #[serde(default)]
    pub org_id: String,

    /// File path or glob (for the `file` backend).
    #[serde(default)]
    pub path: String,

    /// Any extra key-value pairs for future backends.
    #[serde(flatten, default)]
    pub extra: HashMap<String, toml::Value>,
}

impl SourceConfig {
    /// Resolve the backend type, applying auto-detection if the user left
    /// `type` empty.
    pub fn resolved_type(&self) -> &str {
        if !self.backend_type.is_empty() {
            return &self.backend_type;
        }
        if !self.path.is_empty() {
            return "file";
        }
        if self.url.is_empty() {
            return "demo";
        }
        if self.url.contains(":9200")
            || self.url.contains("elastic")
            || self.url.contains("opensearch")
        {
            return "elasticsearch";
        }
        if self.url.contains("amazonaws.com")
            || !self
                .extra
                .get("region")
                .map(|v| v.to_string())
                .unwrap_or_default()
                .is_empty()
        {
            return "cloudwatch";
        }
        if self.url.contains(":3100") || self.url.contains("loki") {
            return "loki";
        }
        // Default fallback — treat as Loki since it's the primary backend.
        "loki"
    }

    /// Build a [`ConnectionConfig`] from the flat fields.
    pub fn to_connection_config(&self) -> ConnectionConfig {
        let auth = if !self.token.is_empty() {
            Some(AuthConfig::Bearer {
                token: self.token.clone(),
            })
        } else if !self.username.is_empty() {
            Some(AuthConfig::Basic {
                username: self.username.clone(),
                password: self.password.clone(),
            })
        } else {
            None
        };

        let mut extra = HashMap::new();
        if !self.org_id.is_empty() {
            extra.insert("org_id".to_string(), self.org_id.clone());
        }
        if !self.path.is_empty() {
            extra.insert("path".to_string(), self.path.clone());
        }
        // Pass through any extra keys (command, container, selector,
        // namespace, pod, etc.) so backends can read them.
        for (k, v) in &self.extra {
            if let Some(s) = v.as_str() {
                extra.insert(k.clone(), s.to_string());
            } else {
                extra.insert(k.clone(), v.to_string());
            }
        }

        ConnectionConfig {
            url: self.url.clone(),
            auth,
            extra,
        }
    }
}

// ── Top-level config ────────────────────────────────────────────────

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

    /// Named sources that can be switched at runtime via the source picker.
    #[serde(default)]
    pub sources: Vec<SourceConfig>,

    /// Team configuration sync settings.
    #[serde(default)]
    pub sync: crate::sync::SyncConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            backend: "demo".to_string(),
            connection: ConnectionConfig::default(),
            display: DisplayConfig::default(),
            keybindings: HashMap::new(),
            sources: Vec::new(),
            sync: crate::sync::SyncConfig::default(),
        }
    }
}

// ── Connection ──────────────────────────────────────────────────────

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

// ── Display ─────────────────────────────────────────────────────────

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
