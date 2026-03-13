//! Serde types for Loki HTTP API JSON responses.
//!
//! These types mirror the JSON structures returned by the Loki HTTP API
//! endpoints (`/loki/api/v1/query_range`, `/loki/api/v1/query`,
//! `/loki/api/v1/labels`, etc.).
//!
//! See: <https://grafana.com/docs/loki/latest/reference/loki-http-api/>

use std::collections::BTreeMap;

use chrono::DateTime;
use serde::Deserialize;

use oxo_core::backend::LogEntry;

/// Top-level response wrapper from Loki.
///
/// All Loki API responses share this structure.
#[derive(Debug, Deserialize)]
pub struct LokiResponse {
    /// Status string — typically "success".
    pub status: String,
    /// The response payload.
    pub data: LokiData,
}

/// The `data` field in a Loki response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LokiData {
    /// The type of result: "streams", "matrix", "vector", or "scalar".
    pub result_type: String,
    /// The actual result entries.
    pub result: Vec<LokiStream>,
}

/// A single stream in a Loki query result.
///
/// Each stream has a set of labels and a list of timestamped log entries.
#[derive(Debug, Deserialize)]
pub struct LokiStream {
    /// The label set for this stream (e.g. `{"job": "api", "level": "error"}`).
    #[serde(rename = "stream")]
    pub labels: BTreeMap<String, String>,
    /// The log entries in this stream.
    ///
    /// Each value is a `[timestamp_nanos_string, log_line]` pair.
    pub values: Vec<(String, String)>,
}

/// Response from `/loki/api/v1/labels`.
#[derive(Debug, Deserialize)]
pub struct LabelsResponse {
    pub status: String,
    pub data: Vec<String>,
}

/// Response from `/loki/api/v1/label/{name}/values`.
#[derive(Debug, Deserialize)]
pub struct LabelValuesResponse {
    pub status: String,
    pub data: Vec<String>,
}

/// Response frame from the `/loki/api/v1/tail` WebSocket endpoint.
///
/// The tail endpoint pushes frames that contain one or more streams.
#[derive(Debug, Deserialize)]
pub struct TailFrame {
    /// The streams in this frame.
    pub streams: Vec<LokiStream>,
    /// Dropped entries info (if Loki is shedding load).
    pub dropped_entries: Option<Vec<DroppedEntry>>,
}

/// Information about entries that Loki dropped during tailing.
#[derive(Debug, Deserialize)]
pub struct DroppedEntry {
    pub labels: BTreeMap<String, String>,
    pub timestamp: String,
}

impl LokiStream {
    /// Convert this Loki stream into a vector of normalized [`LogEntry`] values.
    ///
    /// Loki returns timestamps as nanosecond Unix epoch strings.
    /// This method parses them into proper `DateTime<Utc>` values.
    pub fn into_log_entries(self) -> Vec<LogEntry> {
        self.values
            .into_iter()
            .filter_map(|(ts_str, line)| {
                let ts_nanos: i64 = ts_str.parse().ok()?;
                let secs = ts_nanos / 1_000_000_000;
                let nanos = (ts_nanos % 1_000_000_000) as u32;
                let timestamp = DateTime::from_timestamp(secs, nanos)?;
                Some(LogEntry {
                    timestamp,
                    labels: self.labels.clone(),
                    line,
                    raw: None,
                })
            })
            .collect()
    }
}

impl TailFrame {
    /// Convert all streams in this tail frame into normalized log entries.
    pub fn into_log_entries(self) -> Vec<LogEntry> {
        self.streams
            .into_iter()
            .flat_map(|stream| stream.into_log_entries())
            .collect()
    }
}
