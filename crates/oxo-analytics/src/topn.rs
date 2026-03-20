//! Top-N analysis for log streams.
//!
//! Provides fast aggregation to answer common operational questions:
//!
//! - Which endpoints are the slowest (by extracted latency)?
//! - Which label values produce the most log noise?
//! - Which label values produce the most errors?

use std::collections::HashMap;

use oxo_core::LogEntry;
use regex::Regex;

/// Latency percentiles for a log pattern / endpoint.
#[derive(Debug, Clone)]
pub struct EndpointLatency {
    /// The pattern or endpoint this data belongs to (e.g. `"GET /api/users"`).
    pub pattern: String,
    /// 50th percentile (median) latency in milliseconds.
    pub p50_ms: f64,
    /// 95th percentile latency in milliseconds.
    pub p95_ms: f64,
    /// 99th percentile latency in milliseconds.
    pub p99_ms: f64,
    /// Maximum observed latency in milliseconds.
    pub max_ms: f64,
    /// Number of latency samples.
    pub sample_count: usize,
}

/// Stateless top-N analyzer.
pub struct TopNAnalyzer;

impl TopNAnalyzer {
    /// Extract latencies from log lines and compute per-pattern percentiles.
    ///
    /// Returns the top `n` slowest endpoints/patterns sorted by P95 latency
    /// descending.
    ///
    /// Latency is extracted using common patterns like `duration=45ms`,
    /// `latency: 120ms`, `took 300ms`, etc. The "pattern" is derived from the
    /// first few tokens of the log line (typically the HTTP method + path).
    pub fn slowest_endpoints(entries: &[LogEntry], n: usize) -> Vec<EndpointLatency> {
        let latency_re = Regex::new(
            r"(?i)(?:duration|latency|took|elapsed|time)[=: ]+(\d+(?:\.\d+)?)\s*(?:ms|s|us)",
        )
        .expect("latency regex should compile");

        let mut by_pattern: HashMap<String, Vec<f64>> = HashMap::new();

        for entry in entries {
            if let Some(caps) = latency_re.captures(&entry.line) {
                let raw_value: f64 = match caps.get(1).and_then(|m| m.as_str().parse().ok()) {
                    Some(v) => v,
                    None => continue,
                };

                // Determine the unit and normalize to milliseconds.
                let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                let lower = full_match.to_ascii_lowercase();
                let ms = if lower.ends_with("us") {
                    raw_value / 1000.0
                } else if lower.ends_with('s') && !lower.ends_with("ms") {
                    raw_value * 1000.0
                } else {
                    // Already ms.
                    raw_value
                };

                // Derive a pattern from the first tokens of the line.
                let pattern = extract_pattern(&entry.line);
                by_pattern.entry(pattern).or_default().push(ms);
            }
        }

        let mut results: Vec<EndpointLatency> = by_pattern
            .into_iter()
            .map(|(pattern, mut latencies)| {
                latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let len = latencies.len();
                EndpointLatency {
                    pattern,
                    p50_ms: percentile(&latencies, 50),
                    p95_ms: percentile(&latencies, 95),
                    p99_ms: percentile(&latencies, 99),
                    max_ms: latencies.last().copied().unwrap_or(0.0),
                    sample_count: len,
                }
            })
            .collect();

        results.sort_by(|a, b| {
            b.p95_ms
                .partial_cmp(&a.p95_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(n);
        results
    }

    /// Top N noisiest label values by entry count.
    ///
    /// Returns `(value, count, percentage)` tuples sorted by count descending.
    pub fn noisiest(entries: &[LogEntry], label: &str, n: usize) -> Vec<(String, usize, f64)> {
        let total = entries.len();
        if total == 0 {
            return Vec::new();
        }

        let mut counts: HashMap<String, usize> = HashMap::new();
        for entry in entries {
            if let Some(value) = entry.labels.get(label) {
                *counts.entry(value.clone()).or_default() += 1;
            }
        }

        let mut sorted: Vec<(String, usize, f64)> = counts
            .into_iter()
            .map(|(value, count)| {
                let pct = count as f64 / total as f64 * 100.0;
                (value, count, pct)
            })
            .collect();

        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(n);
        sorted
    }

    /// Top N error-producing label values.
    ///
    /// Only counts entries where the `level` label is `"error"` or `"fatal"`.
    /// Returns `(value, error_count, percentage_of_all_errors)` tuples.
    pub fn top_errors(entries: &[LogEntry], label: &str, n: usize) -> Vec<(String, usize, f64)> {
        let error_entries: Vec<&LogEntry> = entries
            .iter()
            .filter(|e| {
                e.labels
                    .get("level")
                    .map(|l| {
                        let lower = l.to_ascii_lowercase();
                        lower == "error" || lower == "fatal"
                    })
                    .unwrap_or(false)
            })
            .collect();

        let total_errors = error_entries.len();
        if total_errors == 0 {
            return Vec::new();
        }

        let mut counts: HashMap<String, usize> = HashMap::new();
        for entry in &error_entries {
            if let Some(value) = entry.labels.get(label) {
                *counts.entry(value.clone()).or_default() += 1;
            }
        }

        let mut sorted: Vec<(String, usize, f64)> = counts
            .into_iter()
            .map(|(value, count)| {
                let pct = count as f64 / total_errors as f64 * 100.0;
                (value, count, pct)
            })
            .collect();

        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(n);
        sorted
    }
}

/// Extract a coarse pattern from the beginning of a log line.
///
/// Takes the first 3 non-numeric, non-timestamp-like tokens as the pattern
/// key (e.g. `"GET /api/users"`).
fn extract_pattern(line: &str) -> String {
    let tokens: Vec<&str> = line
        .split_whitespace()
        .filter(|t| {
            // Skip tokens that look like timestamps, numbers, or IDs.
            !t.chars().all(|c| {
                c.is_ascii_digit() || c == '.' || c == '-' || c == ':' || c == 'T' || c == 'Z'
            })
        })
        .take(3)
        .collect();

    if tokens.is_empty() {
        "<unknown>".to_string()
    } else {
        tokens.join(" ")
    }
}

/// Compute a percentile value from a sorted slice.
fn percentile(sorted: &[f64], pct: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) * pct / 100).min(sorted.len() - 1);
    sorted[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_entry(line: &str, level: &str, service: &str) -> LogEntry {
        let mut labels = BTreeMap::new();
        labels.insert("level".to_string(), level.to_string());
        labels.insert("service".to_string(), service.to_string());
        LogEntry {
            timestamp: chrono::Utc::now(),
            labels,
            line: line.to_string(),
            raw: None,
        }
    }

    #[test]
    fn latency_extraction_various_formats() {
        let entries = vec![
            make_entry("GET /api/users duration=45ms", "info", "web"),
            make_entry("GET /api/users duration=120ms", "info", "web"),
            make_entry("GET /api/users duration=200ms", "info", "web"),
            make_entry("POST /api/orders latency: 500ms", "info", "web"),
            make_entry("POST /api/orders latency: 800ms", "info", "web"),
        ];

        let results = TopNAnalyzer::slowest_endpoints(&entries, 10);
        assert!(!results.is_empty(), "expected at least one endpoint");

        // All results should have positive latencies.
        for r in &results {
            assert!(r.p50_ms > 0.0, "p50 should be positive: {:?}", r);
            assert!(r.p95_ms >= r.p50_ms, "p95 >= p50: {:?}", r);
            assert!(r.max_ms >= r.p99_ms, "max >= p99: {:?}", r);
        }
    }

    #[test]
    fn percentile_computation() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&values, 50), 5.0); // idx (9*50/100)=4
        assert_eq!(percentile(&values, 95), 9.0); // idx (9*95/100)=8
        assert_eq!(percentile(&values, 99), 9.0); // idx (9*99/100)=8
    }

    #[test]
    fn percentile_empty() {
        assert_eq!(percentile(&[], 50), 0.0);
    }

    #[test]
    fn noisiest_label_values() {
        let entries = vec![
            make_entry("line", "info", "auth"),
            make_entry("line", "info", "auth"),
            make_entry("line", "info", "auth"),
            make_entry("line", "info", "payments"),
            make_entry("line", "info", "frontend"),
        ];

        let top = TopNAnalyzer::noisiest(&entries, "service", 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "auth");
        assert_eq!(top[0].1, 3);
    }

    #[test]
    fn top_errors_by_label() {
        let entries = vec![
            make_entry("line", "error", "payments"),
            make_entry("line", "error", "payments"),
            make_entry("line", "error", "auth"),
            make_entry("line", "info", "payments"),
            make_entry("line", "info", "auth"),
        ];

        let top = TopNAnalyzer::top_errors(&entries, "service", 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "payments");
        assert_eq!(top[0].1, 2);
        assert_eq!(top[1].0, "auth");
        assert_eq!(top[1].1, 1);
    }

    #[test]
    fn no_entries_returns_empty() {
        assert!(TopNAnalyzer::noisiest(&[], "service", 5).is_empty());
        assert!(TopNAnalyzer::top_errors(&[], "service", 5).is_empty());
        assert!(TopNAnalyzer::slowest_endpoints(&[], 5).is_empty());
    }
}
