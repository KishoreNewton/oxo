//! Metrics extraction from log entries.
//!
//! Scans log lines for numeric values using configurable regex patterns
//! and extracts them as typed metrics. Also supports direct extraction
//! from JSON-structured log lines.
//!
//! # Default patterns
//!
//! Out of the box, the [`MetricsExtractor`] recognises:
//!
//! - **duration_ms** — `duration=45ms`, `latency: 120`, `response_time=300ms`
//! - **status_code** — `status=200`, `status_code: 404`
//! - **bytes** — `bytes=1024`, `content_length: 2048`
//! - **count** — `count=10`, `total: 42`, `num_items=5`
//!
//! Custom patterns can be added via [`MetricsExtractor::add_pattern`].

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use regex::Regex;

use oxo_core::LogEntry;

/// A single metric value extracted from a log entry.
#[derive(Debug, Clone)]
pub struct LogMetric {
    /// Name of the metric (e.g. "duration_ms", "status_code").
    pub name: String,
    /// The numeric value extracted from the log line.
    pub value: f64,
    /// Timestamp of the originating log entry.
    pub timestamp: DateTime<Utc>,
    /// Labels from the originating log entry.
    pub labels: BTreeMap<String, String>,
}

/// A compiled pattern used to extract a named metric from log lines.
pub struct MetricPattern {
    /// Name for the metric this pattern produces.
    pub name: String,
    /// Compiled regex with a named capture group for the value.
    pub regex: Regex,
    /// The name of the capture group that holds the numeric value.
    pub value_group: String,
}

impl std::fmt::Debug for MetricPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricPattern")
            .field("name", &self.name)
            .field("regex", &self.regex.as_str())
            .field("value_group", &self.value_group)
            .finish()
    }
}

/// Extracts numeric metrics from log entries using regex patterns.
///
/// Initialised with a set of default patterns covering common log formats.
/// Custom patterns can be added with [`add_pattern`](MetricsExtractor::add_pattern).
#[derive(Debug)]
pub struct MetricsExtractor {
    patterns: Vec<MetricPattern>,
}

impl Default for MetricsExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsExtractor {
    /// Create a new extractor with the default built-in patterns.
    pub fn new() -> Self {
        let defaults = vec![
            (
                "duration_ms",
                r"(?i)(?:duration|latency|elapsed|response_time|took)\s*[=:]\s*(?P<value>\d+(?:\.\d+)?)\s*(?:ms|milliseconds?)?",
                "value",
            ),
            (
                "status_code",
                r"(?i)(?:status|status_code|http_status)\s*[=:]\s*(?P<value>\d{3})",
                "value",
            ),
            (
                "bytes",
                r"(?i)(?:bytes|size|content_length|response_size)\s*[=:]\s*(?P<value>\d+)",
                "value",
            ),
            (
                "count",
                r"(?i)(?:count|total|num_|items)\s*[=:]\s*(?P<value>\d+)",
                "value",
            ),
        ];

        let patterns = defaults
            .into_iter()
            .map(|(name, re, group)| MetricPattern {
                name: name.to_string(),
                regex: Regex::new(re).expect("default metric regex should compile"),
                value_group: group.to_string(),
            })
            .collect();

        Self { patterns }
    }

    /// Add a custom metric extraction pattern.
    ///
    /// # Arguments
    ///
    /// * `name` — name for the resulting metric.
    /// * `regex_str` — regex pattern string; must contain a named capture group.
    /// * `value_group` — name of the capture group that holds the numeric value.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `regex_str` fails to compile.
    pub fn add_pattern(
        &mut self,
        name: &str,
        regex_str: &str,
        value_group: &str,
    ) -> Result<(), regex::Error> {
        let regex = Regex::new(regex_str)?;
        self.patterns.push(MetricPattern {
            name: name.to_string(),
            regex,
            value_group: value_group.to_string(),
        });
        Ok(())
    }

    /// Extract metrics from a log entry by applying all configured patterns.
    ///
    /// Each pattern that matches produces a [`LogMetric`]. Multiple patterns
    /// can match the same entry, producing multiple metrics.
    pub fn extract(&self, entry: &LogEntry) -> Vec<LogMetric> {
        let mut results = Vec::new();

        for pattern in &self.patterns {
            if let Some(caps) = pattern.regex.captures(&entry.line) {
                if let Some(m) = caps.name(&pattern.value_group) {
                    if let Ok(value) = m.as_str().parse::<f64>() {
                        results.push(LogMetric {
                            name: pattern.name.clone(),
                            value,
                            timestamp: entry.timestamp,
                            labels: entry.labels.clone(),
                        });
                    }
                }
            }
        }

        results
    }

    /// Extract metrics from a JSON-structured log entry.
    ///
    /// If the entry's line is valid JSON, all numeric fields (both integers
    /// and floats) are extracted as individual metrics. The metric name is
    /// set to the JSON field name.
    ///
    /// Non-numeric fields are ignored.
    pub fn extract_from_json(&self, entry: &LogEntry) -> Vec<LogMetric> {
        let trimmed = entry.line.trim();
        if !trimmed.starts_with('{') {
            return Vec::new();
        }

        let obj: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let map = match obj.as_object() {
            Some(m) => m,
            None => return Vec::new(),
        };

        let mut results = Vec::new();

        for (key, val) in map {
            let numeric = match val {
                serde_json::Value::Number(n) => n.as_f64(),
                _ => None,
            };
            if let Some(value) = numeric {
                results.push(LogMetric {
                    name: key.clone(),
                    value,
                    timestamp: entry.timestamp,
                    labels: entry.labels.clone(),
                });
            }
        }

        results
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(line: &str) -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            labels: BTreeMap::new(),
            line: line.to_string(),
            raw: None,
        }
    }

    fn make_entry_with_labels(line: &str, labels: BTreeMap<String, String>) -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            labels,
            line: line.to_string(),
            raw: None,
        }
    }

    #[test]
    fn extract_duration_ms() {
        let ext = MetricsExtractor::new();
        let entry = make_entry("request completed duration=45ms status=200");
        let metrics = ext.extract(&entry);
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "duration_ms" && (m.value - 45.0).abs() < f64::EPSILON),
            "should extract duration_ms=45"
        );
    }

    #[test]
    fn extract_duration_float() {
        let ext = MetricsExtractor::new();
        let entry = make_entry("latency: 12.5 ms");
        let metrics = ext.extract(&entry);
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "duration_ms" && (m.value - 12.5).abs() < f64::EPSILON),
            "should extract duration_ms=12.5"
        );
    }

    #[test]
    fn extract_status_code() {
        let ext = MetricsExtractor::new();
        let entry = make_entry("status_code=404 method=GET path=/api/users");
        let metrics = ext.extract(&entry);
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "status_code" && (m.value - 404.0).abs() < f64::EPSILON),
            "should extract status_code=404"
        );
    }

    #[test]
    fn extract_bytes() {
        let ext = MetricsExtractor::new();
        let entry = make_entry("response sent bytes=2048 path=/download");
        let metrics = ext.extract(&entry);
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "bytes" && (m.value - 2048.0).abs() < f64::EPSILON),
            "should extract bytes=2048"
        );
    }

    #[test]
    fn extract_count() {
        let ext = MetricsExtractor::new();
        let entry = make_entry("processed items=42 in batch");
        let metrics = ext.extract(&entry);
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "count" && (m.value - 42.0).abs() < f64::EPSILON),
            "should extract count=42"
        );
    }

    #[test]
    fn extract_multiple_metrics() {
        let ext = MetricsExtractor::new();
        let entry = make_entry("duration=100ms status=200 bytes=512");
        let metrics = ext.extract(&entry);
        assert!(
            metrics.len() >= 3,
            "should extract at least 3 metrics, got {}",
            metrics.len()
        );
    }

    #[test]
    fn no_match_plain_text() {
        let ext = MetricsExtractor::new();
        let entry = make_entry("application started successfully");
        let metrics = ext.extract(&entry);
        assert!(metrics.is_empty(), "plain text should not produce metrics");
    }

    #[test]
    fn custom_pattern() {
        let mut ext = MetricsExtractor::new();
        ext.add_pattern("temperature", r"temp[=:]\s*(?P<val>\d+(?:\.\d+)?)", "val")
            .unwrap();

        let entry = make_entry("sensor reading temp=23.5 unit=celsius");
        let metrics = ext.extract(&entry);
        assert!(
            metrics
                .iter()
                .any(|m| m.name == "temperature" && (m.value - 23.5).abs() < f64::EPSILON),
            "should extract custom temperature metric"
        );
    }

    #[test]
    fn extract_json_numeric_fields() {
        let ext = MetricsExtractor::new();
        let entry =
            make_entry(r#"{"duration": 150, "status": 200, "message": "ok", "ratio": 0.95}"#);
        let metrics = ext.extract_from_json(&entry);
        assert_eq!(metrics.len(), 3, "should extract 3 numeric JSON fields");
        assert!(metrics.iter().any(|m| m.name == "duration"));
        assert!(metrics.iter().any(|m| m.name == "status"));
        assert!(metrics.iter().any(|m| m.name == "ratio"));
    }

    #[test]
    fn extract_json_no_numeric() {
        let ext = MetricsExtractor::new();
        let entry = make_entry(r#"{"message": "hello", "level": "info"}"#);
        let metrics = ext.extract_from_json(&entry);
        assert!(
            metrics.is_empty(),
            "non-numeric JSON should produce no metrics"
        );
    }

    #[test]
    fn extract_json_invalid() {
        let ext = MetricsExtractor::new();
        let entry = make_entry("not json at all");
        let metrics = ext.extract_from_json(&entry);
        assert!(metrics.is_empty(), "non-JSON should produce no metrics");
    }

    #[test]
    fn preserves_labels() {
        let ext = MetricsExtractor::new();
        let mut labels = BTreeMap::new();
        labels.insert("service".to_string(), "api".to_string());
        let entry = make_entry_with_labels("duration=50ms", labels.clone());
        let metrics = ext.extract(&entry);
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].labels, labels);
    }
}
