//! # oxo-elasticsearch
//!
//! [Elasticsearch](https://www.elastic.co/elasticsearch) /
//! [OpenSearch](https://opensearch.org/) backend for the **oxo** observability TUI.
//!
//! This crate implements [`oxo_core::LogBackend`] for Elasticsearch and
//! OpenSearch clusters, providing:
//!
//! - Historical log queries via the `_search` API with `query_string` queries
//! - Polling-based live tailing via `search_after` cursor pagination
//! - Field discovery for the filter panel via `_field_caps`
//! - Label value autocomplete via terms aggregations
//! - Health checking via `_cluster/health`
//!
//! ## Authentication
//!
//! Supports HTTP Basic and Bearer token authentication, configured through
//! [`oxo_core::config::ConnectionConfig`].
//!
//! ## Configuration
//!
//! Set the index pattern via `extra.index` in the connection config
//! (defaults to `"*"` if unset). Set `extra.insecure = "true"` to accept
//! self-signed TLS certificates.
//!
//! ## Auto-detection
//!
//! A source URL is recognized as Elasticsearch if it contains `:9200`,
//! `elastic`, or `opensearch`.
//!
//! ## Example
//!
//! ```no_run
//! use oxo_core::config::ConnectionConfig;
//! use oxo_core::LogBackend;
//! use oxo_elasticsearch::ElasticsearchBackend;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut config = ConnectionConfig::default();
//! config.url = "http://localhost:9200".to_string();
//!
//! let backend = ElasticsearchBackend::from_config(&config)?;
//! backend.health().await?;
//!
//! let labels = backend.labels().await?;
//! println!("Fields: {labels:?}");
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod response;
pub mod tail;

use async_trait::async_trait;
use tokio::sync::mpsc;

use oxo_core::backend::{LogBackend, LogEntry, TailHandle};
use oxo_core::config::ConnectionConfig;
use oxo_core::error::BackendError;
use oxo_core::query::TimeRange;

use crate::client::ElasticsearchClient;

/// Elasticsearch / OpenSearch backend implementation.
///
/// Holds an HTTP client for Elasticsearch API calls and the configured
/// index pattern used as the default search target.
pub struct ElasticsearchBackend {
    /// The HTTP client for Elasticsearch API calls.
    client: ElasticsearchClient,
    /// Stored connection config (needed for spawning tail tasks).
    _config: ConnectionConfig,
}

impl ElasticsearchBackend {
    /// Create a new Elasticsearch backend from an existing client.
    pub fn with_client(
        client: ElasticsearchClient,
        config: ConnectionConfig,
    ) -> Self {
        Self {
            client,
            _config: config,
        }
    }
}

/// Check whether a URL looks like an Elasticsearch / OpenSearch endpoint.
///
/// Used by the source auto-detection logic to determine the backend type
/// when the user has not explicitly specified one.
pub fn looks_like_elasticsearch(url: &str) -> bool {
    url.contains(":9200") || url.contains("elastic") || url.contains("opensearch")
}

#[async_trait]
impl LogBackend for ElasticsearchBackend {
    fn name(&self) -> &str {
        "Elasticsearch"
    }

    async fn query(
        &self,
        query: &str,
        range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        let index = self.client.index();

        let hits = self
            .client
            .search(index, query, range.start, range.end, limit)
            .await?;

        let mut entries: Vec<LogEntry> =
            hits.into_iter().map(|hit| hit.into_log_entry()).collect();

        // Sort by timestamp descending (most recent first), matching Loki's
        // default behavior and what the TUI expects.
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(entries)
    }

    async fn tail(
        &self,
        query: &str,
        tx: mpsc::UnboundedSender<LogEntry>,
    ) -> Result<TailHandle, BackendError> {
        // Verify connectivity before starting the tail loop.
        self.client.health().await?;

        let client = self.client.clone();
        let index = self.client.index().to_string();
        let query = query.to_string();

        let join_handle = tokio::spawn(async move {
            tail::start_tail(&client, &index, &query, tx).await;
        });

        Ok(TailHandle::new(join_handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        let index = self.client.index();
        self.client.field_caps(index).await
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        let index = self.client.index();
        self.client.field_values(index, label).await
    }

    async fn health(&self) -> Result<(), BackendError> {
        self.client.health().await
    }

    fn from_config(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let client = ElasticsearchClient::new(config)?;
        Ok(Self {
            client,
            _config: config.clone(),
        })
    }
}
