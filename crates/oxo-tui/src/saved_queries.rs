//! Persistent saved queries.
//!
//! Queries are stored in `~/.config/oxo/queries.toml` and loaded/saved
//! on demand. This module is intentionally simple — no async I/O, no
//! background watching. Load once at startup, save on mutation.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A single saved query with a human-readable name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQuery {
    /// Display name for the query.
    pub name: String,
    /// The query string (LogQL, KQL, etc.).
    pub query: String,
}

/// Collection of saved queries, (de)serialized as TOML.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SavedQueries {
    /// The list of saved queries.
    #[serde(default)]
    pub queries: Vec<SavedQuery>,
}

impl SavedQueries {
    /// Load saved queries from disk.
    ///
    /// Returns an empty collection if the file does not exist or cannot be
    /// parsed (errors are silently swallowed to avoid crashing the TUI).
    pub fn load() -> Self {
        let path = Self::config_path();
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }

    /// Persist queries to disk.
    ///
    /// Creates the parent directory if it does not exist.
    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&path, content)
    }

    /// Add a new saved query (duplicate names are allowed).
    pub fn add(&mut self, name: String, query: String) {
        self.queries.push(SavedQuery { name, query });
    }

    /// Remove a saved query by index (no-op if out of bounds).
    pub fn remove(&mut self, index: usize) {
        if index < self.queries.len() {
            self.queries.remove(index);
        }
    }

    /// Returns the path to the config file: `~/.config/oxo/queries.toml`.
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("oxo")
            .join("queries.toml")
    }
}
