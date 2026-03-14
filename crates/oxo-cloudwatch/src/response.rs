//! Serde types for CloudWatch Logs API JSON responses.
//!
//! These types mirror the JSON structures returned by the CloudWatch Logs
//! API actions (`FilterLogEvents`, `DescribeLogGroups`,
//! `DescribeLogStreams`, `GetLogEvents`).
//!
//! See: <https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/>

use serde::Deserialize;

// ── FilterLogEvents ─────────────────────────────────────────────────

/// Response from the `FilterLogEvents` API action.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterLogEventsResponse {
    /// The matched log events.
    #[serde(default)]
    pub events: Vec<FilteredLogEvent>,

    /// Token for paginating results. `None` when all results have been
    /// returned.
    #[serde(default, rename = "nextToken")]
    pub next_token: Option<String>,
}

/// A single event from `FilterLogEvents`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredLogEvent {
    /// The name of the log stream this event belongs to.
    #[serde(default)]
    pub log_stream_name: Option<String>,

    /// The event timestamp as milliseconds since Unix epoch.
    #[serde(default)]
    pub timestamp: Option<i64>,

    /// The log event message.
    #[serde(default)]
    pub message: Option<String>,

    /// A unique identifier for this log event within the stream.
    #[serde(default, rename = "eventId")]
    pub event_id: Option<String>,

    /// The time the event was ingested, in milliseconds since epoch.
    #[serde(default)]
    pub ingestion_time: Option<i64>,
}

// ── DescribeLogGroups ───────────────────────────────────────────────

/// Response from the `DescribeLogGroups` API action.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribeLogGroupsResponse {
    /// The log groups matching the request.
    #[serde(default)]
    pub log_groups: Vec<LogGroup>,

    /// Pagination token.
    #[serde(default, rename = "nextToken")]
    pub next_token: Option<String>,
}

/// A CloudWatch Logs log group.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogGroup {
    /// The name of the log group.
    #[serde(default)]
    pub log_group_name: Option<String>,

    /// The ARN of the log group.
    #[serde(default)]
    pub arn: Option<String>,
}

// ── DescribeLogStreams ───────────────────────────────────────────────

/// Response from the `DescribeLogStreams` API action.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribeLogStreamsResponse {
    /// The log streams matching the request.
    #[serde(default)]
    pub log_streams: Vec<LogStream>,

    /// Pagination token.
    #[serde(default, rename = "nextToken")]
    pub next_token: Option<String>,
}

/// A CloudWatch Logs log stream within a log group.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStream {
    /// The name of the log stream.
    #[serde(default)]
    pub log_stream_name: Option<String>,

    /// The timestamp of the last event in the stream, in milliseconds
    /// since epoch.
    #[serde(default)]
    pub last_event_timestamp: Option<i64>,
}

// ── GetLogEvents ────────────────────────────────────────────────────

/// Response from the `GetLogEvents` API action.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetLogEventsResponse {
    /// The log events in this batch.
    #[serde(default)]
    pub events: Vec<OutputLogEvent>,

    /// Token for fetching the next batch of forward-scrolling events.
    #[serde(default, rename = "nextForwardToken")]
    pub next_forward_token: Option<String>,

    /// Token for fetching the next batch of backward-scrolling events.
    #[serde(default, rename = "nextBackwardToken")]
    pub next_backward_token: Option<String>,
}

/// A single event from `GetLogEvents`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputLogEvent {
    /// The event timestamp in milliseconds since Unix epoch.
    #[serde(default)]
    pub timestamp: Option<i64>,

    /// The log event message.
    #[serde(default)]
    pub message: Option<String>,

    /// The time the event was ingested, in milliseconds since epoch.
    #[serde(default)]
    pub ingestion_time: Option<i64>,
}

// ── Error response ──────────────────────────────────────────────────

/// AWS error response body.
///
/// CloudWatch Logs returns errors as JSON with `__type` and `message` fields.
#[derive(Debug, Deserialize)]
pub struct AwsErrorResponse {
    /// The error type (e.g. `"ResourceNotFoundException"`).
    #[serde(default, rename = "__type")]
    pub error_type: Option<String>,

    /// Human-readable error message.
    #[serde(default)]
    pub message: Option<String>,
}
