//! Plugin registry — discovers, loads, and manages WASM plugins.
//!
//! Plugins are loaded from the plugin directory (`~/.config/oxo/plugins/`).
//! Each plugin is either:
//! - A `.wasm` file with an adjacent `.toml` manifest
//! - A directory containing `plugin.wasm` and `manifest.toml`

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing;

use oxo_core::backend::LogEntry;

use crate::host::WasmHost;
use crate::plugin::{Plugin, PluginKind, PluginManifest};

/// Manages loaded plugins and provides access to them by kind.
pub struct PluginRegistry {
    plugins: HashMap<String, Plugin>,
    host: WasmHost,
    plugin_dir: PathBuf,
}

impl PluginRegistry {
    /// Create a new registry, scanning the default plugin directory.
    pub fn new() -> Result<Self> {
        let plugin_dir = default_plugin_dir();
        let host = WasmHost::new()?;
        let mut registry = Self {
            plugins: HashMap::new(),
            host,
            plugin_dir,
        };
        registry.scan()?;
        Ok(registry)
    }

    /// Create a registry from a specific directory.
    pub fn from_dir(dir: PathBuf) -> Result<Self> {
        let host = WasmHost::new()?;
        let mut registry = Self {
            plugins: HashMap::new(),
            host,
            plugin_dir: dir,
        };
        registry.scan()?;
        Ok(registry)
    }

    /// Scan the plugin directory and load all valid plugins.
    pub fn scan(&mut self) -> Result<()> {
        if !self.plugin_dir.exists() {
            tracing::debug!(
                dir = %self.plugin_dir.display(),
                "plugin directory does not exist, skipping"
            );
            return Ok(());
        }

        let entries = std::fs::read_dir(&self.plugin_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if let Err(e) = self.try_load_plugin(&path) {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to load plugin"
                );
            }
        }

        tracing::info!(
            count = self.plugins.len(),
            "loaded plugins"
        );

        Ok(())
    }

    /// Try to load a plugin from a path (file or directory).
    fn try_load_plugin(&mut self, path: &Path) -> Result<()> {
        if path.is_dir() {
            // Directory plugin: plugin.wasm + manifest.toml
            let wasm_path = path.join("plugin.wasm");
            let manifest_path = path.join("manifest.toml");

            if wasm_path.exists() && manifest_path.exists() {
                let manifest_str = std::fs::read_to_string(&manifest_path)?;
                let manifest: PluginManifest = toml::from_str(&manifest_str)?;
                let wasm_bytes = std::fs::read(&wasm_path)?;

                tracing::info!(
                    name = %manifest.name,
                    version = %manifest.version,
                    kind = ?manifest.kind,
                    "loaded plugin"
                );

                let plugin = Plugin::new(manifest.clone(), wasm_bytes);
                self.plugins.insert(manifest.name.clone(), plugin);
            }
        } else if path.extension().is_some_and(|ext| ext == "wasm") {
            // Single .wasm file: look for adjacent .toml
            let stem = path.file_stem().unwrap_or_default();
            let manifest_path = path.with_extension("toml");

            if manifest_path.exists() {
                let manifest_str = std::fs::read_to_string(&manifest_path)?;
                let manifest: PluginManifest = toml::from_str(&manifest_str)?;
                let wasm_bytes = std::fs::read(path)?;

                tracing::info!(
                    name = %manifest.name,
                    version = %manifest.version,
                    "loaded plugin from file"
                );

                let plugin = Plugin::new(manifest.clone(), wasm_bytes);
                self.plugins.insert(manifest.name.clone(), plugin);
            } else {
                // No manifest — create a default one from the filename.
                let name = stem.to_string_lossy().to_string();
                let manifest = PluginManifest {
                    name: name.clone(),
                    version: "0.0.0".to_string(),
                    description: format!("Plugin loaded from {}", path.display()),
                    kind: PluginKind::Transform,
                    author: String::new(),
                };
                let wasm_bytes = std::fs::read(path)?;
                let plugin = Plugin::new(manifest, wasm_bytes);
                self.plugins.insert(name, plugin);
            }
        }

        Ok(())
    }

    /// Get all loaded plugin names.
    pub fn plugin_names(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Get a plugin by name.
    pub fn get(&self, name: &str) -> Option<&Plugin> {
        self.plugins.get(name)
    }

    /// Enable or disable a plugin.
    pub fn set_enabled(&mut self, name: &str, enabled: bool) {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.enabled = enabled;
        }
    }

    /// Run all enabled transform plugins on a batch of entries.
    pub fn apply_transforms(&self, mut entries: Vec<LogEntry>) -> Vec<LogEntry> {
        for plugin in self.plugins.values() {
            if !plugin.enabled || plugin.manifest.kind != PluginKind::Transform {
                continue;
            }

            match self.host.run_transform(&plugin.wasm_bytes, &entries) {
                Ok(transformed) => entries = transformed,
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.manifest.name,
                        error = %e,
                        "transform plugin failed, skipping"
                    );
                }
            }
        }
        entries
    }

    /// Run all enabled filter plugins on a batch of entries.
    pub fn apply_filters(&self, mut entries: Vec<LogEntry>) -> Vec<LogEntry> {
        for plugin in self.plugins.values() {
            if !plugin.enabled || plugin.manifest.kind != PluginKind::Filter {
                continue;
            }

            match self.host.run_filter(&plugin.wasm_bytes, &entries) {
                Ok(filtered) => entries = filtered,
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.manifest.name,
                        error = %e,
                        "filter plugin failed, skipping"
                    );
                }
            }
        }
        entries
    }

    /// Number of loaded plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether no plugins are loaded.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

/// Default plugin directory: `~/.config/oxo/plugins/`
fn default_plugin_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "oxo")
        .map(|dirs: directories::ProjectDirs| dirs.config_dir().join("plugins"))
        .unwrap_or_else(|| PathBuf::from(".oxo/plugins"))
}
