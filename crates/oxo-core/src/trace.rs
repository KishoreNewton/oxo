//! Trace ID detection and correlation.
//!
//! Automatically finds trace IDs, request IDs, and correlation IDs in log
//! lines and structured data, enabling cross-service log correlation.

use regex::Regex;
use std::sync::LazyLock;

/// The kind of trace/correlation identifier found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceIdKind {
    /// A distributed trace ID (e.g. OpenTelemetry 32-char hex).
    TraceId,
    /// A request ID (typically a UUID).
    RequestId,
    /// A correlation ID (typically a UUID).
    CorrelationId,
    /// A span ID (typically 16-char hex).
    SpanId,
}

/// A detected trace/correlation identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceId {
    /// The raw ID value.
    pub id: String,
    /// What kind of identifier this is.
    pub kind: TraceIdKind,
}

// ---------------------------------------------------------------------------
// Compiled regex patterns (compiled once, reused across calls).
// ---------------------------------------------------------------------------

/// UUID pattern: 8-4-4-4-12 hex chars.
const UUID_RE: &str =
    r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}";

/// 32-char hex (OpenTelemetry trace ID format).
const HEX32_RE: &str = r"[0-9a-fA-F]{32}";

/// 16-char hex (span ID format).
const HEX16_RE: &str = r"[0-9a-fA-F]{16}";

// Key=value style patterns.
static TRACE_ID_KV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:trace_id|traceId|trace-id)\s*[=:]\s*(?P<id>{}|{})",
        HEX32_RE, UUID_RE
    ))
    .unwrap()
});

static REQUEST_ID_KV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:request_id|requestId|req_id)\s*[=:]\s*(?P<id>{}|{})",
        UUID_RE, HEX32_RE
    ))
    .unwrap()
});

static CORRELATION_ID_KV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:correlation_id|correlationId)\s*[=:]\s*(?P<id>{}|{})",
        UUID_RE, HEX32_RE
    ))
    .unwrap()
});

static X_REQUEST_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)X-Request-Id:\s*(?P<id>{})", UUID_RE)).unwrap());

static SPAN_ID_KV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:span_id|spanId)\s*[=:]\s*(?P<id>{})",
        HEX16_RE
    ))
    .unwrap()
});

/// JSON field names that indicate a trace/request/correlation/span ID.
const JSON_TRACE_FIELDS: &[(&str, TraceIdKind)] = &[
    ("trace_id", TraceIdKind::TraceId),
    ("traceId", TraceIdKind::TraceId),
    ("request_id", TraceIdKind::RequestId),
    ("requestId", TraceIdKind::RequestId),
    ("correlationId", TraceIdKind::CorrelationId),
    ("correlation_id", TraceIdKind::CorrelationId),
    ("span_id", TraceIdKind::SpanId),
    ("spanId", TraceIdKind::SpanId),
];

/// Stateless detector that scans log lines for trace/correlation IDs.
pub struct TraceDetector;

impl TraceDetector {
    /// Scan a log line for the first recognisable trace or correlation ID.
    ///
    /// Checks both free-text `key=value` / `key: value` patterns and, if the
    /// line looks like JSON, structured field names.
    pub fn detect(line: &str) -> Option<TraceId> {
        // 1. Try structured JSON detection first (more precise).
        if let Some(found) = Self::detect_json(line) {
            return Some(found);
        }

        // 2. Key=value / header patterns in plain text.
        if let Some(caps) = TRACE_ID_KV_RE.captures(line) {
            return Some(TraceId {
                id: caps["id"].to_string(),
                kind: TraceIdKind::TraceId,
            });
        }

        if let Some(caps) = REQUEST_ID_KV_RE.captures(line) {
            return Some(TraceId {
                id: caps["id"].to_string(),
                kind: TraceIdKind::RequestId,
            });
        }

        if let Some(caps) = CORRELATION_ID_KV_RE.captures(line) {
            return Some(TraceId {
                id: caps["id"].to_string(),
                kind: TraceIdKind::CorrelationId,
            });
        }

        if let Some(caps) = X_REQUEST_ID_RE.captures(line) {
            return Some(TraceId {
                id: caps["id"].to_string(),
                kind: TraceIdKind::RequestId,
            });
        }

        if let Some(caps) = SPAN_ID_KV_RE.captures(line) {
            return Some(TraceId {
                id: caps["id"].to_string(),
                kind: TraceIdKind::SpanId,
            });
        }

        None
    }

    /// Try to extract a trace ID from a JSON object in the log line.
    fn detect_json(line: &str) -> Option<TraceId> {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            return None;
        }

        let obj: serde_json::Value = serde_json::from_str(trimmed).ok()?;
        let map = obj.as_object()?;

        for (field, kind) in JSON_TRACE_FIELDS {
            if let Some(val) = map.get(*field) {
                let id_str = match val {
                    serde_json::Value::String(s) if !s.is_empty() => s.clone(),
                    _ => continue,
                };
                return Some(TraceId {
                    id: id_str,
                    kind: kind.clone(),
                });
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Trace reconstruction
// ---------------------------------------------------------------------------

use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::backend::LogEntry;

/// Status of a reconstructed span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanStatus {
    /// All entries within the span are healthy.
    Ok,
    /// At least one entry indicates an error.
    Error,
}

/// A single span within a reconstructed trace.
///
/// Groups log entries that share the same trace ID and span ID,
/// providing timing and status information.
#[derive(Debug, Clone)]
pub struct TraceSpan {
    /// The trace ID this span belongs to.
    pub trace_id: String,
    /// The span ID (if detected).
    pub span_id: Option<String>,
    /// The parent span ID (if detected).
    pub parent_span_id: Option<String>,
    /// The service that emitted these entries.
    pub service: Option<String>,
    /// The operation being performed.
    pub operation: Option<String>,
    /// Earliest timestamp among the span's entries.
    pub start: DateTime<Utc>,
    /// Latest timestamp among the span's entries.
    pub end: DateTime<Utc>,
    /// Duration from start to end.
    pub duration: chrono::Duration,
    /// Whether any entry in the span indicates an error.
    pub status: SpanStatus,
    /// The log entries belonging to this span.
    pub entries: Vec<LogEntry>,
}

/// A fully reconstructed trace built from correlated log entries.
#[derive(Debug, Clone)]
pub struct ReconstructedTrace {
    /// The trace ID.
    pub trace_id: String,
    /// Spans within this trace, sorted by start time.
    pub spans: Vec<TraceSpan>,
    /// Total duration from the earliest span start to the latest span end.
    pub total_duration: chrono::Duration,
    /// Number of unique services participating in the trace.
    pub service_count: usize,
    /// Number of spans that have error status.
    pub error_count: usize,
}

/// Regex for detecting parent_span_id in log lines.
static PARENT_SPAN_ID_KV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:parent_span_id|parentSpanId|parent_id|parentId)\s*[=:]\s*(?P<id>{})",
        HEX16_RE
    ))
    .unwrap()
});

/// JSON field names that may contain a parent span ID.
const JSON_PARENT_SPAN_FIELDS: &[&str] =
    &["parent_span_id", "parentSpanId", "parent_id", "parentId"];

/// Reconstructs traces from a collection of log entries.
///
/// Groups entries by trace ID and span ID, then computes timing,
/// service, operation, and error information for each span.
pub struct TraceReconstructor;

impl TraceReconstructor {
    /// Reconstruct traces from the given log entries.
    ///
    /// 1. Groups entries by trace ID (using [`TraceDetector::detect`]).
    /// 2. Within each trace, groups entries by span ID (if available).
    /// 3. Detects service and operation names from labels.
    /// 4. Computes span timing from min/max timestamps.
    /// 5. Sets error status if any entry contains "error" (case-insensitive)
    ///    or has a `level` label set to "error".
    /// 6. Sorts spans by start time.
    /// 7. Computes total duration, service count, and error count.
    pub fn reconstruct(entries: &[LogEntry]) -> Vec<ReconstructedTrace> {
        // Step 1: Group entries by trace ID.
        let mut by_trace: HashMap<String, Vec<&LogEntry>> = HashMap::new();

        for entry in entries {
            if let Some(trace_id) = TraceDetector::detect(&entry.line) {
                // Only group by actual trace/request/correlation IDs, not span IDs alone.
                match trace_id.kind {
                    TraceIdKind::TraceId | TraceIdKind::RequestId | TraceIdKind::CorrelationId => {
                        by_trace.entry(trace_id.id).or_default().push(entry);
                    }
                    TraceIdKind::SpanId => {
                        // If only a span ID is found (no trace ID), use it as a grouping key.
                        by_trace.entry(trace_id.id).or_default().push(entry);
                    }
                }
            }
        }

        // Step 2-8: Build ReconstructedTrace for each trace group.
        let mut traces: Vec<ReconstructedTrace> = by_trace
            .into_iter()
            .map(|(trace_id, trace_entries)| Self::build_trace(trace_id, &trace_entries))
            .collect();

        // Sort traces by earliest span start time.
        traces.sort_by_key(|t| t.spans.first().map(|s| s.start));

        traces
    }

    /// Build a single `ReconstructedTrace` from entries sharing the same trace ID.
    fn build_trace(trace_id: String, entries: &[&LogEntry]) -> ReconstructedTrace {
        // Group by span_id.
        let mut by_span: HashMap<String, Vec<&LogEntry>> = HashMap::new();

        for entry in entries {
            let span_id =
                Self::extract_span_id(&entry.line).unwrap_or_else(|| "_default".to_string());
            by_span.entry(span_id).or_default().push(entry);
        }

        // Build spans.
        let mut spans: Vec<TraceSpan> = by_span
            .into_iter()
            .map(|(span_key, span_entries)| Self::build_span(&trace_id, &span_key, &span_entries))
            .collect();

        // Sort spans by start time.
        spans.sort_by_key(|s| s.start);

        // Compute aggregate metrics.
        let total_duration = if let (Some(first), Some(last)) = (spans.first(), spans.last()) {
            last.end.signed_duration_since(first.start)
        } else {
            chrono::Duration::zero()
        };

        let mut unique_services: HashSet<String> = HashSet::new();
        let mut error_count = 0;

        for span in &spans {
            if let Some(ref svc) = span.service {
                unique_services.insert(svc.clone());
            }
            if span.status == SpanStatus::Error {
                error_count += 1;
            }
        }

        let service_count = unique_services.len();

        ReconstructedTrace {
            trace_id,
            spans,
            total_duration,
            service_count,
            error_count,
        }
    }

    /// Build a single `TraceSpan` from entries sharing the same span ID.
    fn build_span(trace_id: &str, span_key: &str, entries: &[&LogEntry]) -> TraceSpan {
        let span_id = if span_key == "_default" {
            None
        } else {
            Some(span_key.to_string())
        };

        // Detect parent span ID from the first entry that has one.
        let parent_span_id = entries
            .iter()
            .find_map(|e| Self::extract_parent_span_id(&e.line));

        // Detect service from labels.
        let service = entries
            .iter()
            .find_map(|e| Self::find_label(&e.labels, &["service", "app", "job", "container"]));

        // Detect operation from labels.
        let operation = entries
            .iter()
            .find_map(|e| Self::find_label(&e.labels, &["operation", "op", "method", "endpoint"]));

        // Compute start/end from timestamps.
        let start = entries
            .iter()
            .map(|e| e.timestamp)
            .min()
            .expect("span must have at least one entry");
        let end = entries
            .iter()
            .map(|e| e.timestamp)
            .max()
            .expect("span must have at least one entry");
        let duration = end.signed_duration_since(start);

        // Determine status: error if any line contains "error" (case-insensitive)
        // or has level=error.
        let has_error = entries.iter().any(|e| {
            e.line.to_lowercase().contains("error")
                || e.labels
                    .get("level")
                    .map(|l| l.eq_ignore_ascii_case("error"))
                    .unwrap_or(false)
        });
        let status = if has_error {
            SpanStatus::Error
        } else {
            SpanStatus::Ok
        };

        TraceSpan {
            trace_id: trace_id.to_string(),
            span_id,
            parent_span_id,
            service,
            operation,
            start,
            end,
            duration,
            status,
            entries: entries.iter().map(|e| (*e).clone()).collect(),
        }
    }

    /// Extract a span ID from a log line (key=value or JSON).
    fn extract_span_id(line: &str) -> Option<String> {
        // Try JSON first.
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(map) = obj.as_object() {
                    for field in &["span_id", "spanId"] {
                        if let Some(serde_json::Value::String(s)) = map.get(*field) {
                            if !s.is_empty() {
                                return Some(s.clone());
                            }
                        }
                    }
                }
            }
        }

        // Try key=value pattern.
        SPAN_ID_KV_RE
            .captures(line)
            .map(|caps| caps["id"].to_string())
    }

    /// Extract a parent span ID from a log line (key=value or JSON).
    fn extract_parent_span_id(line: &str) -> Option<String> {
        // Try JSON first.
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(map) = obj.as_object() {
                    for field in JSON_PARENT_SPAN_FIELDS {
                        if let Some(serde_json::Value::String(s)) = map.get(*field) {
                            if !s.is_empty() {
                                return Some(s.clone());
                            }
                        }
                    }
                }
            }
        }

        // Try key=value pattern.
        PARENT_SPAN_ID_KV_RE
            .captures(line)
            .map(|caps| caps["id"].to_string())
    }

    /// Find the first matching label value from a set of candidate label keys.
    fn find_label(labels: &BTreeMap<String, String>, candidates: &[&str]) -> Option<String> {
        for key in candidates {
            if let Some(val) = labels.get(*key) {
                if !val.is_empty() {
                    return Some(val.clone());
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_trace_id_equals() {
        let line = "level=info msg=\"handling request\" trace_id=abcdef01234567890abcdef012345678";
        let result = TraceDetector::detect(line).expect("should detect trace_id");
        assert_eq!(result.kind, TraceIdKind::TraceId);
        assert_eq!(result.id, "abcdef01234567890abcdef012345678");
    }

    #[test]
    fn detect_trace_id_camel_case() {
        let line = "traceId=abcdef01234567890abcdef012345678 service=api";
        let result = TraceDetector::detect(line).expect("should detect traceId");
        assert_eq!(result.kind, TraceIdKind::TraceId);
        assert_eq!(result.id, "abcdef01234567890abcdef012345678");
    }

    #[test]
    fn detect_trace_id_hyphenated() {
        let line = "trace-id=abcdef01234567890abcdef012345678";
        let result = TraceDetector::detect(line).expect("should detect trace-id");
        assert_eq!(result.kind, TraceIdKind::TraceId);
    }

    #[test]
    fn detect_request_id_uuid() {
        let line = "request_id=550e8400-e29b-41d4-a716-446655440000 method=GET path=/api/health";
        let result = TraceDetector::detect(line).expect("should detect request_id");
        assert_eq!(result.kind, TraceIdKind::RequestId);
        assert_eq!(result.id, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn detect_request_id_camel() {
        let line = "requestId=550e8400-e29b-41d4-a716-446655440000";
        let result = TraceDetector::detect(line).expect("should detect requestId");
        assert_eq!(result.kind, TraceIdKind::RequestId);
    }

    #[test]
    fn detect_req_id_shorthand() {
        let line = "req_id=550e8400-e29b-41d4-a716-446655440000 status=200";
        let result = TraceDetector::detect(line).expect("should detect req_id");
        assert_eq!(result.kind, TraceIdKind::RequestId);
    }

    #[test]
    fn detect_correlation_id() {
        let line = "correlation_id=550e8400-e29b-41d4-a716-446655440000 event=order.created";
        let result = TraceDetector::detect(line).expect("should detect correlation_id");
        assert_eq!(result.kind, TraceIdKind::CorrelationId);
        assert_eq!(result.id, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn detect_correlation_id_camel() {
        let line = "correlationId=550e8400-e29b-41d4-a716-446655440000";
        let result = TraceDetector::detect(line).expect("should detect correlationId");
        assert_eq!(result.kind, TraceIdKind::CorrelationId);
    }

    #[test]
    fn detect_x_request_id_header() {
        let line = "X-Request-Id: 550e8400-e29b-41d4-a716-446655440000";
        let result = TraceDetector::detect(line).expect("should detect X-Request-Id header");
        assert_eq!(result.kind, TraceIdKind::RequestId);
        assert_eq!(result.id, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn detect_span_id_kv() {
        let line = "span_id=abcdef0123456789 trace_id=abcdef01234567890abcdef012345678";
        // trace_id regex is checked before span_id, so trace_id should win.
        let result = TraceDetector::detect(line).expect("should detect");
        assert_eq!(result.kind, TraceIdKind::TraceId);
    }

    #[test]
    fn detect_span_id_alone() {
        let line = "span_id=abcdef0123456789 level=debug";
        let result = TraceDetector::detect(line).expect("should detect span_id");
        assert_eq!(result.kind, TraceIdKind::SpanId);
        assert_eq!(result.id, "abcdef0123456789");
    }

    #[test]
    fn detect_json_trace_id() {
        let line =
            r#"{"msg":"request","trace_id":"abcdef01234567890abcdef012345678","level":"info"}"#;
        let result = TraceDetector::detect(line).expect("should detect JSON trace_id");
        assert_eq!(result.kind, TraceIdKind::TraceId);
        assert_eq!(result.id, "abcdef01234567890abcdef012345678");
    }

    #[test]
    fn detect_json_trace_id_camel() {
        let line = r#"{"traceId":"abcdef01234567890abcdef012345678","service":"api"}"#;
        let result = TraceDetector::detect(line).expect("should detect JSON traceId");
        assert_eq!(result.kind, TraceIdKind::TraceId);
    }

    #[test]
    fn detect_json_request_id() {
        let line = r#"{"requestId":"550e8400-e29b-41d4-a716-446655440000","method":"POST"}"#;
        let result = TraceDetector::detect(line).expect("should detect JSON requestId");
        assert_eq!(result.kind, TraceIdKind::RequestId);
        assert_eq!(result.id, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn detect_json_correlation_id() {
        let line = r#"{"correlationId":"550e8400-e29b-41d4-a716-446655440000"}"#;
        let result = TraceDetector::detect(line).expect("should detect JSON correlationId");
        assert_eq!(result.kind, TraceIdKind::CorrelationId);
    }

    #[test]
    fn detect_json_span_id() {
        let line = r#"{"spanId":"abcdef0123456789","op":"db.query"}"#;
        let result = TraceDetector::detect(line).expect("should detect JSON spanId");
        assert_eq!(result.kind, TraceIdKind::SpanId);
        assert_eq!(result.id, "abcdef0123456789");
    }

    #[test]
    fn no_detection_on_plain_line() {
        let line = "GET /api/v1/health 200 1ms";
        assert!(TraceDetector::detect(line).is_none());
    }

    #[test]
    fn no_detection_on_empty() {
        assert!(TraceDetector::detect("").is_none());
    }

    #[test]
    fn detect_with_colon_separator() {
        let line = "trace_id: abcdef01234567890abcdef012345678 service=api";
        let result = TraceDetector::detect(line).expect("should detect with colon");
        assert_eq!(result.kind, TraceIdKind::TraceId);
        assert_eq!(result.id, "abcdef01234567890abcdef012345678");
    }

    #[test]
    fn detect_case_insensitive() {
        let line = "TRACE_ID=abcdef01234567890abcdef012345678";
        let result = TraceDetector::detect(line).expect("should detect case-insensitively");
        assert_eq!(result.kind, TraceIdKind::TraceId);
    }
}
