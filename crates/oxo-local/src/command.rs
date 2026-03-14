//! Command backend — runs an arbitrary command and captures its output.
//!
//! Each line of stdout/stderr becomes a log entry. This covers any source
//! that writes to stdout: `npm run dev`, `python app.py`, `cargo run`, etc.

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

/// Backend that runs a shell command and streams its output.
pub struct CommandBackend {
    command: String,
}

impl CommandBackend {
    fn labels(cmd: &str) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::new();
        labels.insert("source".to_string(), "command".to_string());
        // Use the first word of the command as the "service" label.
        let service = cmd.split_whitespace().next().unwrap_or(cmd);
        labels.insert("service".to_string(), service.to_string());
        labels
    }

    fn guess_level(line: &str) -> &'static str {
        let upper = line.to_ascii_uppercase();
        if upper.contains("FATAL") || upper.contains("CRITICAL") || upper.contains("PANIC") {
            "fatal"
        } else if upper.contains("ERROR") || upper.contains("ERR ") || upper.contains("FAILED") {
            "error"
        } else if upper.contains("WARN") {
            "warn"
        } else if upper.contains("DEBUG") || upper.contains("VERBOSE") {
            "debug"
        } else if upper.contains("TRACE") {
            "trace"
        } else {
            "info"
        }
    }
}

#[async_trait]
impl LogBackend for CommandBackend {
    fn name(&self) -> &str {
        "Command"
    }

    async fn query(
        &self,
        _query: &str,
        _range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        // Run the command and capture its output.
        let output = Command::new("sh")
            .arg("-c")
            .arg(&self.command)
            .output()
            .await
            .map_err(|e| BackendError::Connection(format!("failed to run command: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let base_labels = Self::labels(&self.command);

        let entries: Vec<LogEntry> = stdout
            .lines()
            .chain(stderr.lines())
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
        let cmd = self.command.clone();

        let handle = tokio::spawn(async move {
            let base_labels = CommandBackend::labels(&cmd);

            let mut child = match Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("failed to spawn command '{cmd}': {e}");
                    return;
                }
            };

            // Read stdout in a background task.
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            let tx2 = tx.clone();
            let labels2 = base_labels.clone();

            // Spawn stderr reader.
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
                                    CommandBackend::guess_level(&text).to_string(),
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

            // Read stdout in the main task.
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
                                CommandBackend::guess_level(&text).to_string(),
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

            // Wait for stderr reader and the child process to finish.
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
            "service".to_string(),
            "stream".to_string(),
            "level".to_string(),
        ])
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        let values = match label {
            "source" => vec!["command".to_string()],
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
        // Check if the shell is available.
        Command::new("sh")
            .arg("-c")
            .arg("true")
            .output()
            .await
            .map_err(|e| BackendError::Connection(format!("shell not available: {e}")))?;
        Ok(())
    }

    fn from_config(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let command = config
            .extra
            .get("command")
            .filter(|c| !c.is_empty())
            .cloned()
            .ok_or_else(|| {
                BackendError::Connection("command backend requires 'command' in config".to_string())
            })?;

        Ok(Self { command })
    }
}
