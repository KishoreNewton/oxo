//! The pluggable backend trait and associated types.
//!
//! Every log backend (Loki, Elasticsearch, CloudWatch, etc.) implements the
//! [`LogBackend`] trait defined here. The TUI interacts with backends
//! exclusively through this trait, making it straightforward to add new
//! backends without touching any UI code.
//!
//! # Adding a new backend
//!
//! 1. Create a new crate `oxo-{name}` under `crates/`.
//! 2. Implement [`LogBackend`] for your backend struct.
//! 3. Add a match arm in `oxo-cli/src/main.rs` → `create_backend()`.
//!
//! See `docs/adding-a-backend.md` for a full walkthrough.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::ConnectionConfig;
use crate::error::BackendError;
use crate::query::TimeRange;

/// A single log entry, normalized across all backends.
///
/// Regardless of whether a log line comes from Loki, Elasticsearch, or
/// CloudWatch, it is converted into this common representation before
/// reaching the TUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// When this log line was produced.
    pub timestamp: DateTime<Utc>,

    /// Key-value labels / fields associated with the entry.
    ///
    /// In Loki these are stream labels; in Elasticsearch they are document
    /// fields; in CloudWatch they are log group / stream metadata.
    pub labels: BTreeMap<String, String>,

    /// The log line content.
    pub line: String,

    /// The original, unparsed response from the backend (for "inspect" mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

/// A handle to a running live-tail stream.
///
/// Dropping this handle cancels the background task that feeds log entries
/// into the channel. This is the primary mechanism for stopping a tail.
pub struct TailHandle {
    /// The background task running the tail loop.
    join_handle: JoinHandle<()>,
}

impl TailHandle {
    /// Create a new tail handle wrapping a spawned task.
    pub fn new(join_handle: JoinHandle<()>) -> Self {
        Self { join_handle }
    }

    /// Cancel the tail stream.
    pub fn abort(&self) {
        self.join_handle.abort();
    }

    /// Check if the tail task is still running.
    pub fn is_running(&self) -> bool {
        !self.join_handle.is_finished()
    }
}

impl Drop for TailHandle {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

/// The core trait that every log backend must implement.
///
/// All methods are async and return [`BackendError`] on failure. The TUI
/// holds a `Box<dyn LogBackend>` and calls these methods to fetch data.
///
/// # Contract
///
/// - Implementations must be `Send + Sync` (they may be shared across
///   tokio tasks).
/// - [`tail`](LogBackend::tail) must spawn its own background task and
///   return immediately. The returned [`TailHandle`] controls the
///   lifetime of that task.
/// - All methods should respect reasonable timeouts internally.
#[async_trait]
pub trait LogBackend: Send + Sync {
    /// Human-readable name of this backend (e.g. "Loki", "Elasticsearch").
    fn name(&self) -> &str;

    /// Query historical log entries.
    ///
    /// The backend translates `query` in its native query language
    /// (LogQL for Loki, KQL for Elasticsearch, etc.) and returns up to
    /// `limit` entries within the given `range`.
    async fn query(
        &self,
        query: &str,
        range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError>;

    /// Start a live tail stream.
    ///
    /// New log entries matching `query` are sent into `tx`. The returned
    /// [`TailHandle`] must be kept alive — dropping it cancels the stream.
    async fn tail(
        &self,
        query: &str,
        tx: mpsc::UnboundedSender<LogEntry>,
    ) -> Result<TailHandle, BackendError>;

    /// Return available label names (for autocomplete and the filter panel).
    async fn labels(&self) -> Result<Vec<String>, BackendError>;

    /// Return known values for a given label name.
    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError>;

    /// Perform a health check / connectivity test.
    ///
    /// Returns `Ok(())` if the backend can reach its remote server.
    async fn health(&self) -> Result<(), BackendError>;

    /// Create a new instance of this backend from connection configuration.
    ///
    /// This is used by the CLI to construct backends dynamically.
    fn from_config(config: &ConnectionConfig) -> Result<Self, BackendError>
    where
        Self: Sized;
}
