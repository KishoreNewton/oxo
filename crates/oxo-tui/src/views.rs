//! Persistent saved views.
//!
//! A "view" is a saved combination of query, time range, and optional source —
//! like a browser bookmark for log queries. Views are stored in
//! `~/.config/oxo/views.toml` and loaded/saved on demand.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A single saved view capturing a complete query context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedView {
    /// Human-readable name for this view.
    pub name: String,
    /// The query string (LogQL, KQL, etc.).
    pub query: String,
    /// Time range to apply, in minutes.
    pub time_range_minutes: u64,
    /// Optional source name to switch to when loading this view.
    pub source: Option<String>,
}

/// Collection of saved views, (de)serialized as TOML.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SavedViews {
    /// The list of saved views.
    #[serde(default)]
    pub views: Vec<SavedView>,
}

impl SavedViews {
    /// Load saved views from disk.
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

    /// Persist views to disk.
    ///
    /// Creates the parent directory if it does not exist.
    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = toml::to_string_pretty(self) {
            let _ = std::fs::write(&path, content);
        }
    }

    /// Add a new saved view and persist to disk.
    pub fn add(&mut self, view: SavedView) {
        self.views.push(view);
        self.save();
    }

    /// Remove a saved view by name and persist to disk.
    ///
    /// If multiple views share the same name, removes the first match.
    pub fn remove(&mut self, name: &str) {
        if let Some(pos) = self.views.iter().position(|v| v.name == name) {
            self.views.remove(pos);
            self.save();
        }
    }

    /// Find a saved view by name.
    pub fn get(&self, name: &str) -> Option<&SavedView> {
        self.views.iter().find(|v| v.name == name)
    }

    /// Returns the path to the config file: `~/.config/oxo/views.toml`.
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("oxo")
            .join("views.toml")
    }
}
