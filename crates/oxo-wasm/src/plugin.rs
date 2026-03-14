//! Plugin types and manifest.

use serde::{Deserialize, Serialize};

/// What a plugin does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// Transforms log entries (map operation).
    Transform,
    /// Filters log entries (predicate).
    Filter,
    /// Aggregates log entries into metrics/summaries.
    Aggregate,
}

/// Plugin metadata loaded from the manifest file or embedded in the WASM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin name (e.g. "redact-pii").
    pub name: String,
    /// Semver version string.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// What this plugin does.
    pub kind: PluginKind,
    /// Author name.
    #[serde(default)]
    pub author: String,
}

/// A loaded WASM plugin, ready to execute.
pub struct Plugin {
    /// Plugin metadata.
    pub manifest: PluginManifest,
    /// The compiled WASM module (stored as raw bytes for lazy instantiation).
    pub wasm_bytes: Vec<u8>,
    /// Whether this plugin is currently enabled.
    pub enabled: bool,
}

impl Plugin {
    /// Create a new plugin from manifest and WASM bytes.
    pub fn new(manifest: PluginManifest, wasm_bytes: Vec<u8>) -> Self {
        Self {
            manifest,
            wasm_bytes,
            enabled: true,
        }
    }
}
