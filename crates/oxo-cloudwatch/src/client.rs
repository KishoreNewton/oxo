//! HTTP client for the CloudWatch Logs API.
//!
//! Wraps [`reqwest::Client`] with AWS Signature V4 signing, CloudWatch Logs
//! endpoint construction, and response parsing. Each public method maps to
//! a CloudWatch Logs API action.
//!
//! All requests are JSON-RPC style POST requests to the regional endpoint
//! `https://logs.{region}.amazonaws.com` with:
//! - `Content-Type: application/x-amz-json-1.1`
//! - `X-Amz-Target: Logs_20140328.{Action}`
//!
//! See: <https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/>

use std::time::Duration;

use chrono::Utc;
use reqwest::Client;
use serde_json::json;
use url::Url;

use oxo_core::config::ConnectionConfig;
use oxo_core::error::BackendError;

use crate::response::{
    AwsErrorResponse, DescribeLogGroupsResponse, DescribeLogStreamsResponse,
    FilterLogEventsResponse, GetLogEventsResponse,
};
use crate::signing::{AwsCredentials, sign_request};

/// The AWS service name used in SigV4 signing for CloudWatch Logs.
const SERVICE: &str = "logs";

/// Default AWS region when none is configured.
const DEFAULT_REGION: &str = "us-east-1";

/// HTTP client for communicating with CloudWatch Logs.
///
/// Handles SigV4 request signing, endpoint construction, and response
/// deserialization. All methods return deserialized types from
/// [`crate::response`].
#[derive(Debug, Clone)]
pub struct CloudWatchClient {
    /// The underlying HTTP client (connection-pooled).
    http: Client,
    /// Regional CloudWatch Logs endpoint URL.
    endpoint: Url,
    /// AWS region (e.g. `"us-east-1"`).
    region: String,
    /// AWS credentials for request signing.
    credentials: AwsCredentials,
    /// The default log group to operate on.
    log_group: Option<String>,
}

impl CloudWatchClient {
    /// Create a new CloudWatch Logs client from connection configuration.
    ///
    /// Credentials are resolved in order:
    /// 1. `extra` map keys: `access_key`, `secret_key`, `session_token`
    /// 2. Environment variables: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`,
    ///    `AWS_SESSION_TOKEN`
    ///
    /// The region is read from `extra.get("region")`, falling back to
    /// `AWS_REGION`, then `AWS_DEFAULT_REGION`, then `us-east-1`.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::Auth`] if no access key or secret key can be
    /// found. Returns [`BackendError::Connection`] if the HTTP client cannot
    /// be built.
    pub fn new(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let region = config
            .extra
            .get("region")
            .cloned()
            .or_else(|| std::env::var("AWS_REGION").ok())
            .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
            .unwrap_or_else(|| DEFAULT_REGION.to_string());

        let access_key = config
            .extra
            .get("access_key")
            .cloned()
            .or_else(|| std::env::var("AWS_ACCESS_KEY_ID").ok())
            .ok_or_else(|| {
                BackendError::Auth(
                    "AWS access key not found: set `access_key` in config or \
                     AWS_ACCESS_KEY_ID env var"
                        .into(),
                )
            })?;

        let secret_key = config
            .extra
            .get("secret_key")
            .cloned()
            .or_else(|| std::env::var("AWS_SECRET_ACCESS_KEY").ok())
            .ok_or_else(|| {
                BackendError::Auth(
                    "AWS secret key not found: set `secret_key` in config or \
                     AWS_SECRET_ACCESS_KEY env var"
                        .into(),
                )
            })?;

        let session_token = config
            .extra
            .get("session_token")
            .cloned()
            .or_else(|| std::env::var("AWS_SESSION_TOKEN").ok());

        let credentials = AwsCredentials {
            access_key,
            secret_key,
            session_token,
        };

        let log_group = config.extra.get("log_group").cloned();

        // Determine the endpoint URL. If the user provided a URL in config,
        // use that (useful for localstack / testing). Otherwise, build the
        // standard regional endpoint.
        let endpoint_str = if config.url.is_empty() {
            format!("https://logs.{region}.amazonaws.com")
        } else {
            config.url.clone()
        };

        let endpoint = Url::parse(&endpoint_str).map_err(|e| {
            BackendError::Connection(format!("invalid CloudWatch endpoint URL: {e}"))
        })?;

        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                BackendError::Connection(format!("failed to build HTTP client: {e}"))
            })?;

        Ok(Self {
            http,
            endpoint,
            region,
            credentials,
            log_group,
        })
    }

    /// Return the configured default log group, if any.
    pub fn log_group(&self) -> Option<&str> {
        self.log_group.as_deref()
    }

    /// Return the configured region.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// Return a reference to the credentials (used by the tail module).
    pub fn credentials(&self) -> &AwsCredentials {
        &self.credentials
    }

    /// Return the endpoint URL (used by the tail module).
    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    // ── API actions ─────────────────────────────────────────────────

    /// Filter log events from a log group.
    ///
    /// Calls the `FilterLogEvents` API action with an optional filter
    /// pattern. Results are returned in chronological order.
    ///
    /// # Arguments
    ///
    /// * `log_group` — The log group name to search.
    /// * `filter_pattern` — Optional CloudWatch filter pattern. Pass `None`
    ///   or empty string to match all events.
    /// * `start_time_ms` — Start of the time range in milliseconds since epoch.
    /// * `end_time_ms` — End of the time range in milliseconds since epoch.
    /// * `limit` — Maximum number of events to return (max 10,000).
    /// * `next_token` — Pagination token from a previous response.
    pub async fn filter_log_events(
        &self,
        log_group: &str,
        filter_pattern: Option<&str>,
        start_time_ms: i64,
        end_time_ms: i64,
        limit: usize,
        next_token: Option<&str>,
    ) -> Result<FilterLogEventsResponse, BackendError> {
        let mut body = json!({
            "logGroupName": log_group,
            "startTime": start_time_ms,
            "endTime": end_time_ms,
            "limit": limit.min(10_000),
        });

        if let Some(pattern) = filter_pattern {
            if !pattern.is_empty() {
                body["filterPattern"] = serde_json::Value::String(pattern.to_string());
            }
        }

        if let Some(token) = next_token {
            body["nextToken"] = serde_json::Value::String(token.to_string());
        }

        let resp_bytes = self
            .send_action("Logs_20140328.FilterLogEvents", &body)
            .await?;

        serde_json::from_slice(&resp_bytes).map_err(|e| {
            BackendError::Query(format!("failed to parse FilterLogEvents response: {e}"))
        })
    }

    /// Describe log groups, optionally filtered by prefix.
    ///
    /// Calls the `DescribeLogGroups` API action.
    pub async fn describe_log_groups(
        &self,
        prefix: Option<&str>,
    ) -> Result<DescribeLogGroupsResponse, BackendError> {
        let mut body = json!({});

        if let Some(prefix) = prefix {
            if !prefix.is_empty() {
                body["logGroupNamePrefix"] = serde_json::Value::String(prefix.to_string());
            }
        }

        let resp_bytes = self
            .send_action("Logs_20140328.DescribeLogGroups", &body)
            .await?;

        serde_json::from_slice(&resp_bytes).map_err(|e| {
            BackendError::Query(format!("failed to parse DescribeLogGroups response: {e}"))
        })
    }

    /// Describe log streams within a log group.
    ///
    /// Calls the `DescribeLogStreams` API action.
    pub async fn describe_log_streams(
        &self,
        log_group: &str,
    ) -> Result<DescribeLogStreamsResponse, BackendError> {
        let body = json!({
            "logGroupName": log_group,
            "orderBy": "LastEventTime",
            "descending": true,
        });

        let resp_bytes = self
            .send_action("Logs_20140328.DescribeLogStreams", &body)
            .await?;

        serde_json::from_slice(&resp_bytes).map_err(|e| {
            BackendError::Query(format!(
                "failed to parse DescribeLogStreams response: {e}"
            ))
        })
    }

    /// Get log events from a specific log stream.
    ///
    /// Calls the `GetLogEvents` API action. Used by the tail implementation
    /// to poll for new events with a forward token.
    pub async fn get_log_events(
        &self,
        log_group: &str,
        log_stream: &str,
        start_time_ms: Option<i64>,
        forward_token: Option<&str>,
    ) -> Result<GetLogEventsResponse, BackendError> {
        let mut body = json!({
            "logGroupName": log_group,
            "logStreamName": log_stream,
            "startFromHead": true,
        });

        if let Some(start) = start_time_ms {
            body["startTime"] = serde_json::Value::Number(start.into());
        }

        if let Some(token) = forward_token {
            body["nextToken"] = serde_json::Value::String(token.to_string());
        }

        let resp_bytes = self
            .send_action("Logs_20140328.GetLogEvents", &body)
            .await?;

        serde_json::from_slice(&resp_bytes).map_err(|e| {
            BackendError::Query(format!("failed to parse GetLogEvents response: {e}"))
        })
    }

    // ── Private helpers ─────────────────────────────────────────────

    /// Send a signed request to a CloudWatch Logs API action.
    ///
    /// Handles SigV4 signing, header construction, error response parsing,
    /// and rate-limit detection.
    async fn send_action(
        &self,
        target: &str,
        body: &serde_json::Value,
    ) -> Result<Vec<u8>, BackendError> {
        let body_bytes = serde_json::to_vec(body)
            .map_err(|e| BackendError::Query(format!("failed to serialize request body: {e}")))?;

        let host = self
            .endpoint
            .host_str()
            .unwrap_or("logs.us-east-1.amazonaws.com");

        let timestamp = Utc::now();

        let headers_to_sign: Vec<(&str, &str)> = vec![
            ("content-type", "application/x-amz-json-1.1"),
            ("host", host),
            ("x-amz-target", target),
        ];

        let signed = sign_request(
            "POST",
            &self.endpoint,
            &headers_to_sign,
            &body_bytes,
            &self.region,
            SERVICE,
            &self.credentials,
            timestamp,
        );

        let mut request = self
            .http
            .post(self.endpoint.as_str())
            .header("Content-Type", "application/x-amz-json-1.1")
            .header("X-Amz-Target", target)
            .header("X-Amz-Date", &signed.amz_date)
            .header("Authorization", &signed.authorization);

        if let Some(ref token) = signed.security_token {
            request = request.header("X-Amz-Security-Token", token);
        }

        let resp = request.body(body_bytes).send().await.map_err(|e| {
            if e.is_timeout() {
                BackendError::Timeout(Duration::from_secs(30))
            } else {
                BackendError::Connection(format!("CloudWatch request failed: {e}"))
            }
        })?;

        let status = resp.status();

        if status.is_success() {
            let bytes = resp.bytes().await.map_err(|e| {
                BackendError::Connection(format!("failed to read response body: {e}"))
            })?;
            return Ok(bytes.to_vec());
        }

        // Handle error responses.
        let body_text = resp.text().await.unwrap_or_default();

        // Rate limiting.
        if status.as_u16() == 429
            || body_text.contains("ThrottlingException")
            || body_text.contains("Throttling")
        {
            return Err(BackendError::RateLimited { retry_after: None });
        }

        // Auth errors.
        if status.as_u16() == 403 || status.as_u16() == 401 {
            let detail = parse_aws_error(&body_text);
            return Err(BackendError::Auth(format!(
                "CloudWatch auth failed (HTTP {status}): {detail}"
            )));
        }

        // All other errors.
        let detail = parse_aws_error(&body_text);
        Err(BackendError::Query(format!(
            "CloudWatch returned HTTP {status}: {detail}"
        )))
    }
}

/// Extract a human-readable error message from an AWS error response body.
fn parse_aws_error(body: &str) -> String {
    if let Ok(err) = serde_json::from_str::<AwsErrorResponse>(body) {
        let error_type = err.error_type.unwrap_or_default();
        let message = err.message.unwrap_or_default();
        if !error_type.is_empty() && !message.is_empty() {
            format!("{error_type}: {message}")
        } else if !message.is_empty() {
            message
        } else if !error_type.is_empty() {
            error_type
        } else {
            body.to_string()
        }
    } else {
        body.to_string()
    }
}
