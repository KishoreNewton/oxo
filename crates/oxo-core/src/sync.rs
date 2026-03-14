//! Team configuration sync via Git.
//!
//! Enables sharing oxo configurations (saved queries, views, alert rules,
//! dashboard layouts) across a team using a Git repository. Configuration
//! is stored in a `.oxo/` directory within the team's repo.
//!
//! # How it works
//!
//! 1. Users set `sync.repo` in their config pointing to a shared Git repo
//! 2. `oxo` clones/pulls the repo on startup to `~/.cache/oxo/team-config/`
//! 3. Team configs are merged with local configs (local takes precedence)
//! 4. Users can push local config changes back to the shared repo
//!
//! # Config example
//!
//! ```toml
//! [sync]
//! enabled = true
//! repo = "git@github.com:myorg/oxo-config.git"
//! branch = "main"
//! auto_pull = true
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Sync configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// Whether team sync is enabled.
    pub enabled: bool,
    /// Git repository URL for the shared config.
    pub repo: String,
    /// Branch to use (default: "main").
    pub branch: String,
    /// Whether to auto-pull on startup.
    pub auto_pull: bool,
}

/// Team config sync manager.
pub struct ConfigSync {
    config: SyncConfig,
    local_path: PathBuf,
}

impl ConfigSync {
    /// Create a new sync manager.
    pub fn new(config: SyncConfig) -> Self {
        let local_path = cache_dir().join("team-config");
        Self { config, local_path }
    }

    /// Pull the latest team config from the remote repo.
    pub fn pull(&self) -> Result<(), String> {
        if !self.config.enabled || self.config.repo.is_empty() {
            return Ok(());
        }

        let branch = if self.config.branch.is_empty() {
            "main"
        } else {
            &self.config.branch
        };

        if self.local_path.join(".git").exists() {
            // Already cloned — pull.
            let output = Command::new("git")
                .args(["pull", "origin", branch])
                .current_dir(&self.local_path)
                .output()
                .map_err(|e| format!("git pull failed: {e}"))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("git pull failed: {stderr}"));
            }
        } else {
            // Clone the repo.
            std::fs::create_dir_all(&self.local_path)
                .map_err(|e| format!("cannot create cache dir: {e}"))?;

            let output = Command::new("git")
                .args([
                    "clone",
                    "--branch",
                    branch,
                    "--depth",
                    "1",
                    &self.config.repo,
                    &self.local_path.to_string_lossy(),
                ])
                .output()
                .map_err(|e| format!("git clone failed: {e}"))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("git clone failed: {stderr}"));
            }
        }

        Ok(())
    }

    /// Push local config changes back to the shared repo.
    pub fn push(&self, message: &str) -> Result<(), String> {
        if !self.config.enabled || self.config.repo.is_empty() {
            return Err("sync is not enabled".to_string());
        }

        if !self.local_path.join(".git").exists() {
            return Err("team config not cloned yet".to_string());
        }

        // Stage all changes.
        run_git(&self.local_path, &["add", "-A"])?;

        // Check if there are any changes.
        let status = run_git(&self.local_path, &["status", "--porcelain"])?;
        if status.trim().is_empty() {
            return Ok(()); // Nothing to push.
        }

        // Commit.
        run_git(&self.local_path, &["commit", "-m", message])?;

        // Push.
        let branch = if self.config.branch.is_empty() {
            "main"
        } else {
            &self.config.branch
        };
        run_git(&self.local_path, &["push", "origin", branch])?;

        Ok(())
    }

    /// Get the path to a team config file (within the synced repo).
    pub fn team_file(&self, relative: &str) -> PathBuf {
        self.local_path.join(relative)
    }

    /// Read a team config file as a string.
    pub fn read_team_file(&self, relative: &str) -> Option<String> {
        let path = self.team_file(relative);
        std::fs::read_to_string(path).ok()
    }

    /// Write a team config file.
    pub fn write_team_file(&self, relative: &str, content: &str) -> Result<(), String> {
        let path = self.team_file(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create dir: {e}"))?;
        }
        std::fs::write(&path, content)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    /// List files in the team config repo.
    pub fn list_team_files(&self, subdir: &str) -> Vec<PathBuf> {
        let dir = self.local_path.join(subdir);
        if !dir.exists() {
            return Vec::new();
        }
        std::fs::read_dir(dir)
            .into_iter()
            .flat_map(|entries| entries.flatten())
            .map(|e| e.path())
            .collect()
    }

    /// Check if the team config is available.
    pub fn is_available(&self) -> bool {
        self.config.enabled && self.local_path.join(".git").exists()
    }
}

/// Run a git command and return stdout as a string.
fn run_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("git {} failed: {e}", args.first().unwrap_or(&"")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git {} failed: {stderr}",
            args.first().unwrap_or(&"")
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Cache directory for oxo: `~/.cache/oxo/`
fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "oxo")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".cache/oxo"))
}
