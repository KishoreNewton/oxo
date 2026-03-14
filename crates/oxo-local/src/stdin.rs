//! Stdin backend — reads log lines piped via standard input.
//!
//! Enables usage like `kubectl logs pod | oxo` or `cat app.log | oxo`.
//! Only activates when stdin is a pipe (not a TTY).

use std::collections::BTreeMap;
use std::io::IsTerminal;

use async_trait::async_trait;
use chrono::Utc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use oxo_core::backend::{LogBackend, LogEntry, TailHandle};
use oxo_core::config::ConnectionConfig;
use oxo_core::error::BackendError;
use oxo_core::query::TimeRange;

/// Backend that reads log lines from standard input.
pub struct StdinBackend;

impl StdinBackend {
    /// Returns `true` if stdin is a pipe (not a TTY).
    pub fn is_available() -> bool {
        !std::io::stdin().is_terminal()
    }

    /// Guess a log level from the line content.
    fn guess_level(line: &str) -> &'static str {
        let upper = line.to_ascii_uppercase();
        if upper.contains("FATAL") || upper.contains("CRITICAL") {
            "fatal"
        } else if upper.contains("ERROR") || upper.contains("ERR ") {
            "error"
        } else if upper.contains("WARN") {
            "warn"
        } else if upper.contains("DEBUG") {
            "debug"
        } else if upper.contains("TRACE") {
            "trace"
        } else {
            "info"
        }
    }

    /// Build default labels for a stdin entry.
    fn base_labels() -> BTreeMap<String, String> {
        let mut labels = BTreeMap::new();
        labels.insert("source".to_string(), "stdin".to_string());
        labels
    }
}

#[async_trait]
impl LogBackend for StdinBackend {
    fn name(&self) -> &str {
        "Stdin"
    }

    async fn query(
        &self,
        _query: &str,
        _range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        let base_labels = Self::base_labels();
        let mut entries = Vec::new();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }
            let mut labels = base_labels.clone();
            labels.insert("level".to_string(), Self::guess_level(&line).to_string());
            entries.push(LogEntry {
                timestamp: Utc::now(),
                labels,
                line,
                raw: None,
            });
            if entries.len() >= limit {
                break;
            }
        }

        Ok(entries)
    }

    async fn tail(
        &self,
        _query: &str,
        tx: mpsc::UnboundedSender<LogEntry>,
    ) -> Result<TailHandle, BackendError> {
        let handle = tokio::spawn(async move {
            let stdin = tokio::io::stdin();
            let reader = BufReader::new(stdin);
            let mut lines = reader.lines();
            let base_labels = StdinBackend::base_labels();

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if line.is_empty() {
                            continue;
                        }
                        let mut labels = base_labels.clone();
                        labels.insert(
                            "level".to_string(),
                            StdinBackend::guess_level(&line).to_string(),
                        );
                        let entry = LogEntry {
                            timestamp: Utc::now(),
                            labels,
                            line,
                            raw: None,
                        };
                        if tx.send(entry).is_err() {
                            return; // Receiver dropped.
                        }
                    }
                    Ok(None) => {
                        // EOF — stdin closed.
                        return;
                    }
                    Err(e) => {
                        tracing::error!("error reading stdin: {e}");
                        return;
                    }
                }
            }
        });

        Ok(TailHandle::new(handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        Ok(vec!["source".to_string(), "level".to_string()])
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        let values = match label {
            "source" => vec!["stdin".to_string()],
            "level" => vec!["trace", "debug", "info", "warn", "error", "fatal"]
                .into_iter()
                .map(String::from)
                .collect(),
            _ => vec![],
        };
        Ok(values)
    }

    async fn health(&self) -> Result<(), BackendError> {
        Ok(())
    }

    fn from_config(_config: &ConnectionConfig) -> Result<Self, BackendError> {
        Ok(Self)
    }
}
