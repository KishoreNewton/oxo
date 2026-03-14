//! File backend — tails local log files.
//!
//! Reads from the end of a file and watches for new lines appended,
//! similar to `tail -f`. Supports a single file path (glob support
//! can be added later).

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::Utc;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::sync::mpsc;

use oxo_core::backend::{LogBackend, LogEntry, TailHandle};
use oxo_core::config::ConnectionConfig;
use oxo_core::error::BackendError;
use oxo_core::query::TimeRange;

/// Backend that tails a local log file.
pub struct FileBackend {
    path: String,
}

impl FileBackend {
    /// Build default labels for a file entry.
    fn labels(path: &str) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::new();
        labels.insert("source".to_string(), "file".to_string());
        labels.insert(
            "filename".to_string(),
            std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string()),
        );
        labels
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
}

#[async_trait]
impl LogBackend for FileBackend {
    fn name(&self) -> &str {
        "File"
    }

    async fn query(
        &self,
        _query: &str,
        _range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        // Read the last N lines from the file.
        let content = tokio::fs::read_to_string(&self.path)
            .await
            .map_err(|e| BackendError::Connection(format!("cannot read {}: {e}", self.path)))?;

        let base_labels = Self::labels(&self.path);
        let entries: Vec<LogEntry> = content
            .lines()
            .rev()
            .take(limit)
            .map(|line| {
                let mut labels = base_labels.clone();
                labels.insert("level".to_string(), Self::guess_level(line).to_string());
                LogEntry {
                    timestamp: Utc::now(),
                    labels,
                    line: line.to_string(),
                    raw: None,
                }
            })
            .collect();

        Ok(entries)
    }

    async fn tail(
        &self,
        _query: &str,
        tx: mpsc::UnboundedSender<LogEntry>,
    ) -> Result<TailHandle, BackendError> {
        let path = self.path.clone();

        let handle = tokio::spawn(async move {
            let base_labels = FileBackend::labels(&path);

            // Open the file and seek to the end.
            let file = match tokio::fs::File::open(&path).await {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!("failed to open {path}: {e}");
                    return;
                }
            };
            let mut reader = BufReader::new(file);
            if let Err(e) = reader.seek(std::io::SeekFrom::End(0)).await {
                tracing::error!("failed to seek in {path}: {e}");
                return;
            }

            let mut line_buf = String::new();
            loop {
                line_buf.clear();
                match reader.read_line(&mut line_buf).await {
                    Ok(0) => {
                        // No new data — poll again after a short delay.
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    Ok(_) => {
                        let line = line_buf.trim_end().to_string();
                        if line.is_empty() {
                            continue;
                        }
                        let mut labels = base_labels.clone();
                        labels.insert(
                            "level".to_string(),
                            FileBackend::guess_level(&line).to_string(),
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
                    Err(e) => {
                        tracing::error!("error reading {path}: {e}");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(TailHandle::new(handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        Ok(vec![
            "source".to_string(),
            "filename".to_string(),
            "level".to_string(),
        ])
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        let values = match label {
            "source" => vec!["file".to_string()],
            "filename" => vec![
                std::path::Path::new(&self.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| self.path.clone()),
            ],
            "level" => vec!["trace", "debug", "info", "warn", "error", "fatal"]
                .into_iter()
                .map(String::from)
                .collect(),
            _ => vec![],
        };
        Ok(values)
    }

    async fn health(&self) -> Result<(), BackendError> {
        if tokio::fs::metadata(&self.path).await.is_ok() {
            Ok(())
        } else {
            Err(BackendError::Connection(format!(
                "file not found: {}",
                self.path
            )))
        }
    }

    fn from_config(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let path = config
            .extra
            .get("path")
            .filter(|p| !p.is_empty())
            .cloned()
            .ok_or_else(|| {
                BackendError::Connection("file backend requires 'path' in config".to_string())
            })?;

        Ok(Self { path })
    }
}
