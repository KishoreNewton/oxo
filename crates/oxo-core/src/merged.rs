//! A meta-backend that fans out to multiple [`LogBackend`]s and merges their
//! results by timestamp.
//!
//! Each entry produced by a child backend is tagged with a `__source__` label
//! so the TUI can show (and filter on) the originating backend.

use std::collections::BTreeSet;

use async_trait::async_trait;
use futures_util::future::join_all;
use tokio::sync::mpsc;

use crate::backend::{LogBackend, LogEntry, TailHandle};
use crate::config::ConnectionConfig;
use crate::error::BackendError;
use crate::query::TimeRange;

/// Label key injected into every entry to identify which child backend
/// produced it.
const SOURCE_LABEL: &str = "__source__";

/// A backend that wraps multiple child backends and merges their output.
///
/// Historical queries are fanned out concurrently and the results are merged
/// by timestamp.  Live tails forward entries from every child into a single
/// shared channel, again tagged with the source backend name.
pub struct MergedBackend {
    backends: Vec<(String, Box<dyn LogBackend>)>,
}

impl MergedBackend {
    /// Construct a `MergedBackend` from a pre-built list of named backends.
    ///
    /// Each tuple is `(human_name, backend)`.  The name is used as the value
    /// of the `__source__` label on every entry.
    pub fn from_backends(backends: Vec<(String, Box<dyn LogBackend>)>) -> Self {
        Self { backends }
    }
}

/// Tag a single entry with the `__source__` label.
fn tag_entry(mut entry: LogEntry, source: &str) -> LogEntry {
    entry
        .labels
        .insert(SOURCE_LABEL.to_string(), source.to_string());
    entry
}

#[async_trait]
impl LogBackend for MergedBackend {
    fn name(&self) -> &str {
        "merged"
    }

    async fn query(
        &self,
        query: &str,
        range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        // Fan out to every child concurrently.
        let futures: Vec<_> = self
            .backends
            .iter()
            .map(|(name, backend)| {
                let name = name.clone();
                async move {
                    let result = backend.query(query, range, limit).await;
                    (name, result)
                }
            })
            .collect();

        let results = join_all(futures).await;

        // Collect entries from all backends that succeeded, tagging each one.
        let mut merged: Vec<LogEntry> = Vec::new();
        let mut last_error: Option<BackendError> = None;

        for (name, result) in results {
            match result {
                Ok(entries) => {
                    for entry in entries {
                        merged.push(tag_entry(entry, &name));
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        // If every backend failed (or none were configured), propagate an error.
        if merged.is_empty() {
            return Err(last_error.unwrap_or_else(|| {
                BackendError::Connection("no backends returned results".to_string())
            }));
        }

        // Sort by timestamp and truncate to the requested limit.
        merged.sort_by_key(|e| e.timestamp);
        merged.truncate(limit);
        Ok(merged)
    }

    async fn tail(
        &self,
        query: &str,
        tx: mpsc::UnboundedSender<LogEntry>,
    ) -> Result<TailHandle, BackendError> {
        // Collect sub-handles so we can keep them alive.
        let mut sub_handles: Vec<TailHandle> = Vec::new();

        for (name, backend) in &self.backends {
            // Each child gets its own channel; a forwarding task re-tags
            // entries and sends them into the shared `tx`.
            let (child_tx, mut child_rx) = mpsc::unbounded_channel::<LogEntry>();
            let handle = backend.tail(query, child_tx).await?;
            sub_handles.push(handle);

            let parent_tx = tx.clone();
            let source_name = name.clone();
            tokio::spawn(async move {
                while let Some(entry) = child_rx.recv().await {
                    let tagged = tag_entry(entry, &source_name);
                    if parent_tx.send(tagged).is_err() {
                        break; // Parent receiver dropped.
                    }
                }
            });
        }

        // The outer handle keeps all sub-handles alive.  When it is dropped
        // every child tail is aborted via `TailHandle::drop`.
        let handle = tokio::spawn(async move {
            // Hold sub-handles until this task is cancelled.
            let _guards = sub_handles;
            // Park forever — cancellation (abort) is the only exit.
            std::future::pending::<()>().await;
        });

        Ok(TailHandle::new(handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        let futures: Vec<_> = self
            .backends
            .iter()
            .map(|(_, backend)| backend.labels())
            .collect();

        let results = join_all(futures).await;

        let mut all: BTreeSet<String> = BTreeSet::new();
        all.insert(SOURCE_LABEL.to_string());

        for labels in results.into_iter().flatten() {
            all.extend(labels);
        }

        Ok(all.into_iter().collect())
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        if label == SOURCE_LABEL {
            return Ok(self.backends.iter().map(|(name, _)| name.clone()).collect());
        }

        let futures: Vec<_> = self
            .backends
            .iter()
            .map(|(_, backend)| backend.label_values(label))
            .collect();

        let results = join_all(futures).await;

        let mut all: BTreeSet<String> = BTreeSet::new();
        for values in results.into_iter().flatten() {
            all.extend(values);
        }

        Ok(all.into_iter().collect())
    }

    async fn health(&self) -> Result<(), BackendError> {
        let futures: Vec<_> = self
            .backends
            .iter()
            .map(|(_, backend)| backend.health())
            .collect();

        let results = join_all(futures).await;

        // Ok if ANY backend is healthy.
        let mut last_error: Option<BackendError> = None;
        for result in results {
            if result.is_ok() {
                return Ok(());
            }
            last_error = Some(result.unwrap_err());
        }

        Err(last_error
            .unwrap_or_else(|| BackendError::Connection("no backends configured".to_string())))
    }

    fn from_config(_config: &ConnectionConfig) -> Result<Self, BackendError>
    where
        Self: Sized,
    {
        Err(BackendError::Connection(
            "MergedBackend cannot be created from config; use MergedBackend::from_backends() instead"
                .to_string(),
        ))
    }
}
