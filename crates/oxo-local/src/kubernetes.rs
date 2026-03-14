//! Kubernetes backend — streams pod logs via `kubectl`.
//!
//! Supports targeting by pod name or label selector. Requires `kubectl`
//! to be available and configured on the system.

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

/// Backend that streams logs from Kubernetes pods.
pub struct KubernetesBackend {
    /// Namespace to query. Defaults to "default".
    namespace: String,
    /// Pod name or pattern (if set, takes priority over selector).
    pod: String,
    /// Label selector (e.g. "app=api").
    selector: String,
    /// Container name within the pod (optional).
    container: String,
}

impl KubernetesBackend {
    fn labels(namespace: &str) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::new();
        labels.insert("source".to_string(), "kubernetes".to_string());
        labels.insert("namespace".to_string(), namespace.to_string());
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

    /// Build the kubectl command arguments for fetching logs.
    fn kubectl_args(&self, follow: bool, tail: Option<usize>) -> Vec<String> {
        let mut args = vec!["logs".to_string()];

        if follow {
            args.push("-f".to_string());
        }

        if let Some(n) = tail {
            args.push(format!("--tail={n}"));
        }

        args.push(format!("--namespace={}", self.namespace));

        if !self.pod.is_empty() {
            args.push(self.pod.clone());
        } else if !self.selector.is_empty() {
            args.push(format!("--selector={}", self.selector));
        }

        if !self.container.is_empty() {
            args.push(format!("--container={}", self.container));
        }

        // Prefix each line with the pod name for multi-pod selectors.
        if !self.selector.is_empty() {
            args.push("--prefix=true".to_string());
        }

        args
    }
}

#[async_trait]
impl LogBackend for KubernetesBackend {
    fn name(&self) -> &str {
        "Kubernetes"
    }

    async fn query(
        &self,
        _query: &str,
        _range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        let args = self.kubectl_args(false, Some(limit));
        let output = Command::new("kubectl")
            .args(&args)
            .output()
            .await
            .map_err(|e| BackendError::Connection(format!("failed to run kubectl: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BackendError::Connection(format!(
                "kubectl logs failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let base_labels = Self::labels(&self.namespace);

        let entries: Vec<LogEntry> = stdout
            .lines()
            .rev()
            .take(limit)
            .map(|line| {
                let mut labels = base_labels.clone();
                // If --prefix is used, lines look like "pod-name line content".
                if !self.selector.is_empty() {
                    if let Some((pod, _)) = line.split_once(' ') {
                        labels.insert("pod".to_string(), pod.to_string());
                    }
                }
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
        let args = self.kubectl_args(true, Some(100));
        let namespace = self.namespace.clone();
        let has_selector = !self.selector.is_empty();

        let handle = tokio::spawn(async move {
            let base_labels = KubernetesBackend::labels(&namespace);

            let mut child = match Command::new("kubectl")
                .args(&args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("failed to spawn kubectl logs: {e}");
                    return;
                }
            };

            if let Some(stdout) = child.stdout.take() {
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
                            if has_selector {
                                if let Some((pod, _)) = text.split_once(' ') {
                                    labels.insert("pod".to_string(), pod.to_string());
                                }
                            }
                            labels.insert(
                                "level".to_string(),
                                KubernetesBackend::guess_level(&text).to_string(),
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

            let _ = child.wait().await;
        });

        Ok(TailHandle::new(handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        let mut labels = vec![
            "source".to_string(),
            "namespace".to_string(),
            "level".to_string(),
        ];
        if !self.selector.is_empty() {
            labels.push("pod".to_string());
        }
        Ok(labels)
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        let values = match label {
            "source" => vec!["kubernetes".to_string()],
            "namespace" => vec![self.namespace.clone()],
            "level" => vec!["trace", "debug", "info", "warn", "error", "fatal"]
                .into_iter()
                .map(String::from)
                .collect(),
            _ => vec![],
        };
        Ok(values)
    }

    async fn health(&self) -> Result<(), BackendError> {
        let output = Command::new("kubectl")
            .args(["cluster-info"])
            .output()
            .await
            .map_err(|e| BackendError::Connection(format!("kubectl not available: {e}")))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(BackendError::Connection(
                "kubectl cluster-info failed — check your kubeconfig".to_string(),
            ))
        }
    }

    fn from_config(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let namespace = config
            .extra
            .get("namespace")
            .cloned()
            .unwrap_or_else(|| "default".to_string());

        let pod = config.extra.get("pod").cloned().unwrap_or_default();
        let selector = config.extra.get("selector").cloned().unwrap_or_default();
        let container = config.extra.get("container").cloned().unwrap_or_default();

        if pod.is_empty() && selector.is_empty() {
            return Err(BackendError::Connection(
                "kubernetes backend requires either 'pod' or 'selector' in config".to_string(),
            ));
        }

        Ok(Self {
            namespace,
            pod,
            selector,
            container,
        })
    }
}
