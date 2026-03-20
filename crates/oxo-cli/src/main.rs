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
    /// Backend to use: "demo", "loki", etc. Demo generates fake logs.
    #[arg(short, long, default_value = "demo")]
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

    /// AWS region for CloudWatch.
    #[arg(long)]
    region: Option<String>,

    /// Elasticsearch/OpenSearch index pattern.
    #[arg(long)]
    index: Option<String>,

    /// CloudWatch log group.
    #[arg(long)]
    log_group: Option<String>,

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
    let (mut config, config_content) = load_config(&cli)?;

    // Auto-detect stdin pipe mode
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() && config.backend == "demo" {
        config.backend = "stdin".to_string();
    }

    tracing::info!(
        backend = %config.backend,
        url = %config.connection.url,
        "starting oxo"
    );

    // Create the backend.
    let backend = create_backend(&config)?;

    // Build a backend factory so the TUI can switch sources at runtime.
    let factory: Option<oxo_tui::app::BackendFactory> = if config.sources.is_empty() {
        None
    } else {
        Some(Box::new(
            |backend_type: &str, conn: &oxo_core::config::ConnectionConfig| {
                let cfg = oxo_core::config::AppConfig {
                    backend: backend_type.to_string(),
                    connection: conn.clone(),
                    ..Default::default()
                };
                create_backend(&cfg)
            },
        ))
    };

    // Build engine channels.
    let engine_channels = setup_engines(&cli, config_content.as_deref());

    // Create and run the app.
    let sources = config.sources.clone();
    let mut app = oxo_tui::app::App::new(
        backend,
        config.display.clone(),
        cli.query,
        factory,
        sources,
        engine_channels,
    );

    app.run().await?;

    Ok(())
}

/// Load configuration from file and apply CLI overrides.
///
/// Returns the parsed config and the raw file content (for subsystem parsing).
fn load_config(cli: &Cli) -> Result<(AppConfig, Option<String>)> {
    // Start with defaults.
    let (mut config, raw_content) = if let Some(ref path) = cli.config {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let cfg = toml::from_str::<AppConfig>(&content)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;
        (cfg, Some(content))
    } else {
        // Try the default config location.
        let default_path = default_config_path();
        if let Some(path) = default_path {
            if path.exists() {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let cfg = toml::from_str::<AppConfig>(&content).unwrap_or_default();
                (cfg, Some(content))
            } else {
                (AppConfig::default(), None)
            }
        } else {
            (AppConfig::default(), None)
        }
    };

    // Merge per-project config (oxo.toml) if found.
    let mut raw_content = raw_content;
    if let Some(project_path) = find_project_config() {
        if let Ok(content) = std::fs::read_to_string(&project_path) {
            if let Ok(project_cfg) = toml::from_str::<AppConfig>(&content) {
                // Merge: project overrides global.
                if !project_cfg.backend.is_empty() && project_cfg.backend != "demo" {
                    config.backend = project_cfg.backend;
                }
                if project_cfg.connection.url != "http://localhost:3100" {
                    config.connection = project_cfg.connection;
                }
                if !project_cfg.sources.is_empty() {
                    config.sources = project_cfg.sources;
                }
                // Merge keybindings: project overrides global per-key.
                for (k, v) in project_cfg.keybindings {
                    config.keybindings.insert(k, v);
                }

                tracing::info!(
                    path = %project_path.display(),
                    "loaded per-project config"
                );
            }

            // Include project config in the raw content so subsystems
            // (alerts, etc.) can also pick up project-level sections.
            raw_content = match raw_content {
                Some(existing) => Some(format!("{}\n{}", existing, content)),
                None => Some(content),
            };
        }
    }

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
    if let Some(ref region) = cli.region {
        config
            .connection
            .extra
            .insert("region".to_string(), region.clone());
    }
    if let Some(ref index) = cli.index {
        config
            .connection
            .extra
            .insert("index".to_string(), index.clone());
    }
    if let Some(ref log_group) = cli.log_group {
        config
            .connection
            .extra
            .insert("log_group".to_string(), log_group.clone());
    }

    Ok((config, raw_content))
}

/// Create a backend instance based on the configuration.
fn create_backend(config: &AppConfig) -> Result<Box<dyn LogBackend>> {
    match config.backend.as_str() {
        #[cfg(feature = "demo")]
        "demo" => {
            let backend = oxo_demo::DemoBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "loki")]
        "loki" => {
            let backend = oxo_loki::LokiBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "local")]
        "file" => {
            let backend = oxo_local::FileBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "local")]
        "command" => {
            let backend = oxo_local::CommandBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "local")]
        "docker" => {
            let backend = oxo_local::DockerBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "local")]
        "kubernetes" | "k8s" => {
            let backend = oxo_local::KubernetesBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "elasticsearch")]
        "elasticsearch" | "es" | "opensearch" => {
            let backend = oxo_elasticsearch::ElasticsearchBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "cloudwatch")]
        "cloudwatch" | "cw" => {
            let backend = oxo_cloudwatch::CloudWatchBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "local")]
        "stdin" | "pipe" => {
            let backend = oxo_local::StdinBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        other => anyhow::bail!(
            "unknown backend: \"{other}\". Available backends: {}",
            available_backends().join(", ")
        ),
    }
}

/// List available backends based on compiled features.
fn available_backends() -> Vec<&'static str> {
    vec![
        #[cfg(feature = "demo")]
        "demo",
        #[cfg(feature = "loki")]
        "loki",
        #[cfg(feature = "local")]
        "file",
        #[cfg(feature = "local")]
        "command",
        #[cfg(feature = "local")]
        "docker",
        #[cfg(feature = "local")]
        "kubernetes",
        #[cfg(feature = "elasticsearch")]
        "elasticsearch",
        #[cfg(feature = "cloudwatch")]
        "cloudwatch",
        #[cfg(feature = "local")]
        "stdin",
    ]
}

/// Set up the alert and analytics engines, returning channel endpoints for the TUI.
///
/// Parses the `[alerts]` section from the raw config content (if available),
/// creates engine instances, and spawns them as tokio tasks.
fn setup_engines(_cli: &Cli, config_content: Option<&str>) -> oxo_tui::app::EngineChannels {
    let mut channels = oxo_tui::app::EngineChannels::default();

    // ── Alert engine ─────────────────────────────────────────────────
    #[cfg(feature = "alert")]
    {
        // Parse alert config from the [alerts] section of the config file.
        #[derive(serde::Deserialize, Default)]
        struct AlertWrapper {
            #[serde(default)]
            alerts: oxo_alert::config::AlertConfig,
        }

        let alert_config = config_content
            .and_then(|content| toml::from_str::<AlertWrapper>(content).ok())
            .map(|w| w.alerts)
            .unwrap_or_default();

        if alert_config.enabled {
            let (entry_tx, entry_rx) = tokio::sync::mpsc::unbounded_channel();
            let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();

            let engine = oxo_alert::engine::AlertEngine::new(alert_config, event_tx);
            tokio::spawn(engine.run(entry_rx));

            channels.alert_entry_tx = Some(entry_tx);
            channels.alert_event_rx = Some(event_rx);

            tracing::info!("alert engine started");
        }
    }

    // ── Analytics engine ─────────────────────────────────────────────
    #[cfg(feature = "analytics")]
    {
        let (entry_tx, entry_rx) = tokio::sync::mpsc::unbounded_channel();
        let (snapshot_tx, snapshot_rx) = tokio::sync::mpsc::unbounded_channel();

        let engine = oxo_analytics::engine::AnalyticsEngine::new(snapshot_tx);
        tokio::spawn(engine.run(entry_rx));

        channels.analytics_entry_tx = Some(entry_tx);
        channels.analytics_snapshot_rx = Some(snapshot_rx);

        tracing::info!("analytics engine started");
    }

    channels
}

/// Search for an `oxo.toml` project config file in the current directory
/// and all parent directories, stopping at the filesystem root.
///
/// This enables per-project configuration: dropping an `oxo.toml` in a
/// repository root automatically applies project-specific settings
/// (backend, sources, keybindings, etc.) when running `oxo` from
/// anywhere within that project tree.
fn find_project_config() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("oxo.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
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
