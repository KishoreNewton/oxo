//! # oxo
//!
//! **A terminal UI for log aggregation and observability — k9s for your logs.**
//!
//! oxo connects to log backends (Loki, Elasticsearch, CloudWatch) and
//! provides real-time tailing, filtering, and visualization directly in
//! your terminal. No browser, no SaaS bill, just your logs.
//!
//! ## Usage
//!
//! ```sh
//! # Tail all logs from a local Loki instance
//! oxo --url http://localhost:3100
//!
//! # Tail with a specific query
//! oxo --url http://loki:3100 --query '{job="api"} |= "error"'
//!
//! # Use a config file
//! oxo --config ~/.config/oxo/config.toml
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;

use oxo_core::backend::LogBackend;
use oxo_core::config::{AppConfig, AuthConfig};

/// oxo — a terminal UI for log aggregation and observability.
#[derive(Parser, Debug)]
#[command(
    name = "oxo",
    version,
    about = "A terminal UI for log aggregation and observability — k9s for your logs",
    long_about = "oxo connects to log backends (Loki, Elasticsearch, CloudWatch) and provides \
                  real-time tailing, filtering, and visualization directly in your terminal."
)]
struct Cli {
    /// Backend to use (e.g. "loki", "elasticsearch").
    #[arg(short, long, default_value = "loki")]
    backend: String,

    /// Backend URL (e.g. "http://localhost:3100").
    #[arg(short, long)]
    url: Option<String>,

    /// Initial LogQL / query string to start tailing.
    #[arg(short, long)]
    query: Option<String>,

    /// Path to the configuration file.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Username for basic authentication.
    #[arg(long)]
    username: Option<String>,

    /// Password for basic authentication.
    #[arg(long)]
    password: Option<String>,

    /// Bearer token for authentication.
    #[arg(long)]
    token: Option<String>,

    /// Loki tenant ID for multi-tenant deployments.
    #[arg(long)]
    org_id: Option<String>,

    /// Enable debug logging (writes to ~/.local/state/oxo/oxo.log).
    #[arg(long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging.
    setup_logging(cli.debug)?;

    // Load configuration (file → CLI overrides).
    let config = load_config(&cli)?;

    tracing::info!(
        backend = %config.backend,
        url = %config.connection.url,
        "starting oxo"
    );

    // Create the backend.
    let backend = create_backend(&config)?;

    // Create and run the app.
    let mut app = oxo_tui::app::App::new(backend, config.display.clone(), cli.query);

    app.run().await?;

    Ok(())
}

/// Load configuration from file and apply CLI overrides.
fn load_config(cli: &Cli) -> Result<AppConfig> {
    // Start with defaults.
    let mut config = if let Some(ref path) = cli.config {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        toml::from_str::<AppConfig>(&content)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?
    } else {
        // Try the default config location.
        let default_path = default_config_path();
        if let Some(path) = default_path {
            if path.exists() {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                toml::from_str::<AppConfig>(&content).unwrap_or_default()
            } else {
                AppConfig::default()
            }
        } else {
            AppConfig::default()
        }
    };

    // Apply CLI overrides.
    config.backend = cli.backend.clone();

    if let Some(ref url) = cli.url {
        config.connection.url = url.clone();
    }

    // Auth overrides.
    if let (Some(user), Some(pass)) = (&cli.username, &cli.password) {
        config.connection.auth = Some(AuthConfig::Basic {
            username: user.clone(),
            password: pass.clone(),
        });
    } else if let Some(token) = &cli.token {
        config.connection.auth = Some(AuthConfig::Bearer {
            token: token.clone(),
        });
    }

    // Extra backend-specific overrides.
    if let Some(ref org_id) = cli.org_id {
        config
            .connection
            .extra
            .insert("org_id".to_string(), org_id.clone());
    }

    Ok(config)
}

/// Create a backend instance based on the configuration.
fn create_backend(config: &AppConfig) -> Result<Box<dyn LogBackend>> {
    match config.backend.as_str() {
        #[cfg(feature = "loki")]
        "loki" => {
            let backend = oxo_loki::LokiBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        // Future backends:
        // #[cfg(feature = "elasticsearch")]
        // "elasticsearch" => { ... }
        //
        // #[cfg(feature = "cloudwatch")]
        // "cloudwatch" => { ... }
        other => anyhow::bail!(
            "unknown backend: \"{other}\". Available backends: {}",
            available_backends().join(", ")
        ),
    }
}

/// List available backends based on compiled features.
fn available_backends() -> Vec<&'static str> {
    vec![
        #[cfg(feature = "loki")]
        "loki",
    ]
}

/// Get the default config file path (`~/.config/oxo/config.toml`).
fn default_config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "oxo").map(|dirs| dirs.config_dir().join("config.toml"))
}

/// Set up tracing/logging.
///
/// In debug mode, writes structured logs to a file. Otherwise, logging
/// is effectively disabled (only errors go to stderr).
fn setup_logging(debug: bool) -> Result<()> {
    let filter = if debug {
        EnvFilter::new("oxo=debug,oxo_loki=debug,oxo_tui=debug,oxo_core=debug")
    } else {
        EnvFilter::new("warn")
    };

    if debug {
        // Write logs to a file so they don't interfere with the TUI.
        let log_dir = directories::ProjectDirs::from("", "", "oxo")
            .map(|dirs| dirs.state_dir().unwrap_or(dirs.data_dir()).to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        std::fs::create_dir_all(&log_dir)?;
        let log_file = std::fs::File::create(log_dir.join("oxo.log"))?;

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(log_file)
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    }

    Ok(())
}
