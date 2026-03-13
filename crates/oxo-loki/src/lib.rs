//! # oxo-loki
//!
//! [Grafana Loki](https://grafana.com/oss/loki/) backend for the **oxo** observability TUI.
//!
//! This crate implements [`oxo_core::LogBackend`] for Loki, providing:
//!
//! - Historical log queries via the HTTP API (`/loki/api/v1/query_range`)
//! - Real-time log tailing via WebSocket (`/loki/api/v1/tail`)
//! - Label discovery for the filter panel
//! - Health checking via `/ready`
//!
//! ## Authentication
//!
//! Supports HTTP Basic and Bearer token authentication, configured through
//! [`oxo_core::config::ConnectionConfig`]. For multi-tenant Loki deployments,
//! set `org_id` in the connection's `extra` map.
//!
//! ## Example
//!
//! ```no_run
//! use oxo_core::config::ConnectionConfig;
//! use oxo_core::LogBackend;
//! use oxo_loki::LokiBackend;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = ConnectionConfig::default(); // localhost:3100
//! let backend = LokiBackend::from_config(&config)?;
//!
//! // Check connectivity
//! backend.health().await?;
//!
//! // Discover available labels
//! let labels = backend.labels().await?;
//! println!("Labels: {labels:?}");
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod query;
pub mod response;
pub mod tail;

use async_trait::async_trait;
use tokio::sync::mpsc;

use oxo_core::backend::{LogBackend, LogEntry, TailHandle};
use oxo_core::config::ConnectionConfig;
use oxo_core::error::BackendError;
use oxo_core::query::TimeRange;

use crate::client::LokiClient;

/// Grafana Loki backend implementation.
///
/// Holds an HTTP client for query/label operations and the connection
/// config needed to establish WebSocket tail streams.
pub struct LokiBackend {
    /// The HTTP client for Loki API calls.
    client: LokiClient,
    /// Stored connection config (needed for spawning tail WebSockets).
    _config: ConnectionConfig,
}

impl LokiBackend {
    /// Create a new Loki backend from an existing client.
    pub fn with_client(client: LokiClient, config: ConnectionConfig) -> Self {
        Self {
            client,
            _config: config,
        }
    }
}

#[async_trait]
impl LogBackend for LokiBackend {
    fn name(&self) -> &str {
        "Loki"
    }

    async fn query(
        &self,
        query: &str,
        range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        // Convert DateTime<Utc> to nanosecond timestamps for the Loki API.
        let start_ns = range
            .start
            .timestamp_nanos_opt()
            .ok_or_else(|| BackendError::Query("start timestamp out of range".into()))?;
        let end_ns = range
            .end
            .timestamp_nanos_opt()
            .ok_or_else(|| BackendError::Query("end timestamp out of range".into()))?;

        let streams = self
            .client
            .query_range(query, start_ns, end_ns, limit)
            .await?;

        let mut entries: Vec<LogEntry> = streams
            .into_iter()
            .flat_map(|s| s.into_log_entries())
            .collect();

        // Sort by timestamp descending (most recent first).
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(entries)
    }

    async fn tail(
        &self,
        query: &str,
        tx: mpsc::UnboundedSender<LogEntry>,
    ) -> Result<TailHandle, BackendError> {
        let base_url = self.client.base_url().clone();
        let auth = self.client.auth().clone();
        let org_id = self.client.org_id().map(|s| s.to_string());
        let query = query.to_string();

        let join_handle = tokio::spawn(async move {
            if let Err(e) = tail::start_tail(&base_url, &query, &auth, org_id.as_deref(), tx).await
            {
                tracing::error!("tail stream ended with error: {e}");
            }
        });

        Ok(TailHandle::new(join_handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        self.client.labels().await
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        self.client.label_values(label).await
    }

    async fn health(&self) -> Result<(), BackendError> {
        self.client.health().await
    }

    fn from_config(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let client = LokiClient::new(config)?;
        Ok(Self {
            client,
            _config: config.clone(),
        })
    }
}
