//! # oxo-cloudwatch
//!
//! [AWS CloudWatch Logs](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/)
//! backend for the **oxo** observability TUI.
//!
//! This crate implements [`oxo_core::LogBackend`] for CloudWatch Logs, providing:
//!
//! - Historical log queries via `FilterLogEvents`
//! - Polling-based live tailing via repeated `FilterLogEvents` calls
//! - Label discovery (`log_group` via `DescribeLogGroups`, `log_stream`
//!   via `DescribeLogStreams`)
//! - Health checking via `DescribeLogGroups` with limit 1
//!
//! ## Authentication
//!
//! AWS credentials are resolved in order:
//! 1. Config `extra` map keys: `access_key`, `secret_key`, `session_token`
//! 2. Environment variables: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`,
//!    `AWS_SESSION_TOKEN`
//!
//! All requests are signed with AWS Signature Version 4 using a lightweight
//! manual implementation (no AWS SDK dependency).
//!
//! ## Auto-detection
//!
//! A source is recognized as CloudWatch if its URL contains
//! `amazonaws.com` or its config has a `region` extra key.
//!
//! ## Example
//!
//! ```no_run
//! use oxo_core::config::ConnectionConfig;
//! use oxo_core::LogBackend;
//! use oxo_cloudwatch::CloudWatchBackend;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut config = ConnectionConfig::default();
//! config.extra.insert("region".into(), "us-west-2".into());
//! config.extra.insert("log_group".into(), "/aws/lambda/my-fn".into());
//!
//! let backend = CloudWatchBackend::from_config(&config)?;
//! backend.health().await?;
//!
//! let labels = backend.labels().await?;
//! println!("Labels: {labels:?}");
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod response;
pub mod signing;
pub mod tail;

mod convert;

use async_trait::async_trait;
use tokio::sync::mpsc;

use oxo_core::backend::{LogBackend, LogEntry, TailHandle};
use oxo_core::config::ConnectionConfig;
use oxo_core::error::BackendError;
use oxo_core::query::TimeRange;

use crate::client::CloudWatchClient;
use crate::convert::filtered_event_to_log_entry;

/// AWS CloudWatch Logs backend implementation.
///
/// Holds an HTTP client with SigV4 signing for CloudWatch Logs API calls.
pub struct CloudWatchBackend {
    /// The signed HTTP client for CloudWatch Logs API calls.
    client: CloudWatchClient,
    /// Stored connection config for reference.
    _config: ConnectionConfig,
}

impl CloudWatchBackend {
    /// Create a new CloudWatch backend from an existing client.
    pub fn with_client(client: CloudWatchClient, config: ConnectionConfig) -> Self {
        Self {
            client,
            _config: config,
        }
    }

    /// Return `true` if the given [`ConnectionConfig`] looks like it should
    /// use the CloudWatch backend.
    ///
    /// Matches if the URL contains `amazonaws.com` or the `extra` map
    /// contains a `region` key.
    pub fn auto_detect(config: &ConnectionConfig) -> bool {
        config.url.contains("amazonaws.com") || config.extra.contains_key("region")
    }

    /// Resolve the effective log group from the client config or return an
    /// error if none is configured.
    fn require_log_group(&self) -> Result<String, BackendError> {
        self.client
            .log_group()
            .map(|s| s.to_string())
            .ok_or_else(|| {
                BackendError::Query(
                    "no log_group configured: set `log_group` in the source config".into(),
                )
            })
    }
}

#[async_trait]
impl LogBackend for CloudWatchBackend {
    fn name(&self) -> &str {
        "CloudWatch"
    }

    async fn query(
        &self,
        query: &str,
        range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        let log_group = self.require_log_group()?;

        let start_ms = range.start.timestamp_millis();
        let end_ms = range.end.timestamp_millis();

        // Use the query text as a CloudWatch filter pattern. An empty query
        // matches all events.
        let filter_pattern = if query.is_empty() { None } else { Some(query) };

        let mut entries = Vec::new();
        let mut next_token: Option<String> = None;

        // Paginate through results until we reach the limit or run out.
        loop {
            let response = self
                .client
                .filter_log_events(
                    &log_group,
                    filter_pattern,
                    start_ms,
                    end_ms,
                    limit.saturating_sub(entries.len()),
                    next_token.as_deref(),
                )
                .await?;

            for event in &response.events {
                if let Some(entry) = filtered_event_to_log_entry(event, &log_group) {
                    entries.push(entry);
                    if entries.len() >= limit {
                        break;
                    }
                }
            }

            if entries.len() >= limit {
                break;
            }

            match response.next_token {
                Some(token) if !token.is_empty() => next_token = Some(token),
                _ => break,
            }
        }

        // Sort by timestamp descending (most recent first), matching Loki behavior.
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(entries)
    }

    async fn tail(
        &self,
        query: &str,
        tx: mpsc::UnboundedSender<LogEntry>,
    ) -> Result<TailHandle, BackendError> {
        let log_group = self.require_log_group()?;

        let filter_pattern = if query.is_empty() {
            None
        } else {
            Some(query.to_string())
        };

        // Clone the client for the spawned task. The client is designed
        // to be cheaply cloneable (reqwest::Client uses Arc internally).
        let client = self.client.clone();

        let join_handle = tokio::spawn(async move {
            tail::start_tail(&client, &log_group, filter_pattern.as_deref(), tx).await;
        });

        Ok(TailHandle::new(join_handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        // CloudWatch Logs doesn't have a label discovery API like Loki.
        // We expose the structural dimensions as labels.
        Ok(vec![
            "log_group".to_string(),
            "log_stream".to_string(),
            "level".to_string(),
        ])
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        match label {
            "log_group" => {
                let resp = self.client.describe_log_groups(None).await?;
                let values: Vec<String> = resp
                    .log_groups
                    .into_iter()
                    .filter_map(|g| g.log_group_name)
                    .collect();
                Ok(values)
            }
            "log_stream" => {
                let log_group = self.require_log_group()?;
                let resp = self.client.describe_log_streams(&log_group).await?;
                let values: Vec<String> = resp
                    .log_streams
                    .into_iter()
                    .filter_map(|s| s.log_stream_name)
                    .collect();
                Ok(values)
            }
            _ => {
                // For labels we don't natively support (like "level"),
                // return an empty list. The TUI will handle this gracefully.
                Ok(Vec::new())
            }
        }
    }

    async fn health(&self) -> Result<(), BackendError> {
        // Use DescribeLogGroups as a lightweight health check. If we can
        // successfully make a signed API call and get a response, the
        // connection and credentials are working.
        self.client.describe_log_groups(None).await?;
        Ok(())
    }

    fn from_config(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let client = CloudWatchClient::new(config)?;
        Ok(Self {
            client,
            _config: config.clone(),
        })
    }
}

/// Check whether a [`ConnectionConfig`] should use the CloudWatch backend.
///
/// This is a free function for use in match arms when constructing backends
/// dynamically.
pub fn is_cloudwatch_config(config: &ConnectionConfig) -> bool {
    CloudWatchBackend::auto_detect(config)
}
