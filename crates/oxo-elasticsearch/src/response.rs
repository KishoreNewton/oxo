//! Serde types for Elasticsearch / OpenSearch JSON responses.
//!
//! These types mirror the JSON structures returned by the Elasticsearch
//! Search, Field Capabilities, Cluster Health, and Aggregation APIs.
//!
//! See: <https://www.elastic.co/guide/en/elasticsearch/reference/current/rest-apis.html>

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use oxo_core::backend::LogEntry;

// ── Search API ──────────────────────────────────────────────────────

/// Top-level response from `POST /{index}/_search`.
#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    /// The matching documents.
    pub hits: HitsWrapper,
}

/// Wrapper around the hits array and total count.
#[derive(Debug, Deserialize)]
pub struct HitsWrapper {
    /// Total number of matching documents.
    pub total: TotalHits,
    /// The actual hit documents.
    #[serde(default)]
    pub hits: Vec<EsHit>,
}

/// Total hit count returned by Elasticsearch.
#[derive(Debug, Deserialize)]
pub struct TotalHits {
    /// The count of matching documents.
    pub value: u64,
}

/// A single document hit from Elasticsearch.
#[derive(Debug, Deserialize)]
pub struct EsHit {
    /// The index this document belongs to.
    #[serde(rename = "_index")]
    pub index: String,
    /// The document source (all fields).
    #[serde(rename = "_source")]
    pub source: Value,
    /// Sort values used for pagination via `search_after`.
    #[serde(default)]
    pub sort: Option<Vec<Value>>,
}

impl EsHit {
    /// Convert this Elasticsearch hit into a normalized [`LogEntry`].
    ///
    /// Timestamp extraction tries these fields in order:
    /// 1. `@timestamp` (standard ECS field)
    /// 2. `timestamp`
    ///
    /// The log line is taken from the first available field:
    /// 1. `message` (ECS standard)
    /// 2. `msg`
    /// 3. `log`
    /// 4. Falls back to serializing the entire `_source` as JSON.
    ///
    /// All string-valued top-level fields from `_source` are flattened
    /// into the `labels` map for filtering in the TUI.
    pub fn into_log_entry(self) -> LogEntry {
        let source_obj = self.source.as_object();

        // ── Timestamp ───────────────────────────────────────────────
        let timestamp = source_obj
            .and_then(|obj| {
                obj.get("@timestamp")
                    .or_else(|| obj.get("timestamp"))
                    .and_then(|v| v.as_str())
            })
            .and_then(|s| parse_es_timestamp(s))
            .unwrap_or_else(Utc::now);

        // ── Log line ────────────────────────────────────────────────
        let line = source_obj
            .and_then(|obj| {
                obj.get("message")
                    .or_else(|| obj.get("msg"))
                    .or_else(|| obj.get("log"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| {
                serde_json::to_string(&self.source).unwrap_or_default()
            });

        // ── Labels (flatten string-valued fields) ───────────────────
        let mut labels = BTreeMap::new();
        if let Some(obj) = source_obj {
            for (key, val) in obj {
                // Skip the fields we already extracted.
                if key == "message" || key == "msg" || key == "log" {
                    continue;
                }
                if let Some(s) = val.as_str() {
                    labels.insert(key.clone(), s.to_string());
                } else if val.is_number() || val.is_boolean() {
                    labels.insert(key.clone(), val.to_string());
                }
            }
        }

        // Always include the index as a label.
        labels.insert("_index".to_string(), self.index);

        LogEntry {
            timestamp,
            labels,
            line,
            raw: Some(self.source),
        }
    }
}

/// Parse an Elasticsearch timestamp string into a `DateTime<Utc>`.
///
/// Supports ISO 8601 formats with and without fractional seconds,
/// as well as epoch milliseconds (numeric strings).
fn parse_es_timestamp(s: &str) -> Option<DateTime<Utc>> {
    // Try ISO 8601 with timezone (e.g. "2024-01-15T10:30:00.000Z").
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }

    // Try ISO 8601 without timezone (assume UTC).
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(dt.and_utc());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt.and_utc());
    }

    // Try epoch milliseconds as a string.
    if let Ok(millis) = s.parse::<i64>() {
        let secs = millis / 1000;
        let nanos = ((millis % 1000) * 1_000_000) as u32;
        return DateTime::from_timestamp(secs, nanos);
    }

    None
}

// ── Field Capabilities API ──────────────────────────────────────────

/// Response from `GET /{index}/_field_caps?fields=*`.
#[derive(Debug, Deserialize)]
pub struct FieldCapsResponse {
    /// Map of field name → type name → capability info.
    pub fields: HashMap<String, HashMap<String, FieldCap>>,
}

/// Capabilities of a single field type mapping.
#[derive(Debug, Deserialize)]
pub struct FieldCap {
    /// The Elasticsearch field type (e.g. "keyword", "text", "long").
    #[serde(rename = "type")]
    pub type_: String,
    /// Whether the field is searchable.
    pub searchable: bool,
}

// ── Cluster Health API ──────────────────────────────────────────────

/// Response from `GET /_cluster/health`.
#[derive(Debug, Deserialize)]
pub struct ClusterHealthResponse {
    /// Cluster health status: "green", "yellow", or "red".
    pub status: String,
}

// ── Aggregation API ─────────────────────────────────────────────────

/// Response from a terms aggregation query.
#[derive(Debug, Deserialize)]
pub struct AggResponse {
    /// The aggregation results (absent if the query had no aggregations).
    pub aggregations: Option<AggWrapper>,
}

/// Wrapper holding the named aggregation.
#[derive(Debug, Deserialize)]
pub struct AggWrapper {
    /// The "values" aggregation bucket list.
    pub values: BucketsWrapper,
}

/// Wrapper around the buckets array.
#[derive(Debug, Deserialize)]
pub struct BucketsWrapper {
    /// Individual aggregation buckets.
    pub buckets: Vec<Bucket>,
}

/// A single aggregation bucket.
#[derive(Debug, Deserialize)]
pub struct Bucket {
    /// The bucket key (the distinct field value).
    pub key: Value,
}
