//! Docker backend — streams logs from a Docker container.
//!
//! Uses `docker logs -f --tail <N>` under the hood. Requires `docker` CLI
//! to be available on the system PATH.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::Utc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use oxo_core::backend::{LogBackend, LogEntry, TailHandle};
use oxo_core::config::ConnectionConfig;
use oxo_core::error::BackendError;
use oxo_core::query::TimeRange;

/// Backend that streams logs from a Docker container.
pub struct DockerBackend {
    container: String,
}

impl DockerBackend {
    fn labels(container: &str) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::new();
        labels.insert("source".to_string(), "docker".to_string());
        labels.insert("container".to_string(), container.to_string());
        labels
    }

    fn guess_level(line: &str) -> &'static str {
        let upper = line.to_ascii_uppercase();
        if upper.contains("FATAL") || upper.contains("CRITICAL") || upper.contains("PANIC") {
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
impl LogBackend for DockerBackend {
    fn name(&self) -> &str {
        "Docker"
    }

    async fn query(
        &self,
        _query: &str,
        _range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        let output = Command::new("docker")
            .args(["logs", "--tail", &limit.to_string(), &self.container])
            .output()
            .await
            .map_err(|e| BackendError::Connection(format!("failed to run docker logs: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BackendError::Connection(format!(
                "docker logs failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let base_labels = Self::labels(&self.container);

        let entries: Vec<LogEntry> = stdout
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
        let container = self.container.clone();

        let handle = tokio::spawn(async move {
            let base_labels = DockerBackend::labels(&container);

            let mut child = match Command::new("docker")
                .args(["logs", "-f", "--tail", "100", &container])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("failed to spawn docker logs for '{container}': {e}");
                    return;
                }
            };

            // Docker sends logs to both stdout and stderr — merge them.
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            let tx2 = tx.clone();
            let labels2 = base_labels.clone();

            let stderr_task = stderr.map(|stderr| {
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stderr);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line).await {
                            Ok(0) => break,
                            Ok(_) => {
                                let text = line.trim_end().to_string();
                                if text.is_empty() {
                                    continue;
                                }
                                let mut labels = labels2.clone();
                                labels.insert("stream".to_string(), "stderr".to_string());
                                labels.insert(
                                    "level".to_string(),
                                    DockerBackend::guess_level(&text).to_string(),
                                );
                                let entry = LogEntry {
                                    timestamp: Utc::now(),
                                    labels,
                                    line: text,
                                    raw: None,
                                };
                                if tx2.send(entry).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                })
            });

            if let Some(stdout) = stdout {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let text = line.trim_end().to_string();
                            if text.is_empty() {
                                continue;
                            }
                            let mut labels = base_labels.clone();
                            labels.insert("stream".to_string(), "stdout".to_string());
                            labels.insert(
                                "level".to_string(),
                                DockerBackend::guess_level(&text).to_string(),
                            );
                            let entry = LogEntry {
                                timestamp: Utc::now(),
                                labels,
                                line: text,
                                raw: None,
                            };
                            if tx.send(entry).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }

            if let Some(task) = stderr_task {
                let _ = task.await;
            }
            let _ = child.wait().await;
        });

        Ok(TailHandle::new(handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        Ok(vec![
            "source".to_string(),
            "container".to_string(),
            "stream".to_string(),
            "level".to_string(),
        ])
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        let values = match label {
            "source" => vec!["docker".to_string()],
            "container" => vec![self.container.clone()],
            "stream" => vec!["stdout".to_string(), "stderr".to_string()],
            "level" => vec!["trace", "debug", "info", "warn", "error", "fatal"]
                .into_iter()
                .map(String::from)
                .collect(),
            _ => vec![],
        };
        Ok(values)
    }

    async fn health(&self) -> Result<(), BackendError> {
        let output = Command::new("docker")
            .args(["inspect", "--format", "{{.State.Running}}", &self.container])
            .output()
            .await
            .map_err(|e| BackendError::Connection(format!("docker not available: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim() == "true" {
            Ok(())
        } else {
            Err(BackendError::Connection(format!(
                "container '{}' is not running",
                self.container
            )))
        }
    }

    fn from_config(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let container = config
            .extra
            .get("container")
            .filter(|c| !c.is_empty())
            .cloned()
            .ok_or_else(|| {
                BackendError::Connection(
                    "docker backend requires 'container' in config".to_string(),
                )
            })?;

        Ok(Self { container })
    }
}
