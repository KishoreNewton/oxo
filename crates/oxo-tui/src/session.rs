//! Session persistence — save and restore TUI state across runs.
//!
//! The session file lives at `~/.config/oxo/session.toml` and stores the
//! user's active tabs, time range, source selection, and label filters so
//! they can be restored on the next launch.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Serializable snapshot of the TUI session state.
#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    /// LogQL / query strings for each open tab.
    pub tab_queries: Vec<String>,
    /// Index of the currently active tab.
    pub active_tab: usize,
    /// Active time range in minutes.
    pub time_range_minutes: u64,
    /// Name of the currently selected source (if any).
    pub active_source: Option<String>,
    /// Active label filters as `(label, value)` pairs.
    pub filters: Vec<(String, String)>,
}

impl Session {
    /// Path to the session file (`~/.config/oxo/session.toml`).
    pub fn session_path() -> PathBuf {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("oxo");
        dir.join("session.toml")
    }

    /// Persist the session to disk.
    ///
    /// Creates the parent directory if it does not exist.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::session_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml = toml::to_string_pretty(self)?;
        std::fs::write(&path, toml)?;
        Ok(())
    }

    /// Load a previously saved session, returning `None` if the file does
    /// not exist or cannot be parsed.
    pub fn load() -> Option<Session> {
        let path = Self::session_path();
        let contents = std::fs::read_to_string(&path).ok()?;
        toml::from_str(&contents).ok()
    }

    /// Delete the session file if it exists.
    pub fn delete() {
        let path = Self::session_path();
        let _ = std::fs::remove_file(path);
    }
}
