//! Conversion helpers for turning CloudWatch API responses into normalized
//! [`LogEntry`] values.
//!
//! This is a private module shared by the query path (`lib.rs`) and the
//! tail path (`tail.rs`) to avoid duplicating the event→LogEntry mapping.

use std::collections::BTreeMap;

use chrono::DateTime;
use serde_json::json;

use oxo_core::backend::LogEntry;

use crate::response::FilteredLogEvent;

/// Convert a [`FilteredLogEvent`] into a normalized [`LogEntry`].
///
/// Returns `None` if the event has no timestamp (which is required for a
/// meaningful log entry).
pub fn filtered_event_to_log_entry(event: &FilteredLogEvent, log_group: &str) -> Option<LogEntry> {
    let timestamp_ms = event.timestamp?;
    let secs = timestamp_ms / 1_000;
    let nanos = ((timestamp_ms % 1_000) * 1_000_000) as u32;
    let timestamp = DateTime::from_timestamp(secs, nanos)?;

    let line = event.message.clone().unwrap_or_default();

    let mut labels = BTreeMap::new();
    labels.insert("log_group".to_string(), log_group.to_string());
    if let Some(ref stream) = event.log_stream_name {
        labels.insert("log_stream".to_string(), stream.clone());
    }

    let raw = Some(json!({
        "eventId": event.event_id,
        "logStreamName": event.log_stream_name,
        "timestamp": event.timestamp,
        "ingestionTime": event.ingestion_time,
        "message": event.message,
    }));

    Some(LogEntry {
        timestamp,
        labels,
        line,
        raw,
    })
}
