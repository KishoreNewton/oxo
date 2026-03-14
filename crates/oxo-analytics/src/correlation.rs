//! Error correlation analysis.
//!
//! When error rates spike, this module identifies which label values changed
//! the most between a baseline window and a comparison window. For example,
//! if the `service=payments` label suddenly accounts for 80% of errors (up
//! from 5%), it will surface at the top of the correlation results.

use std::collections::HashMap;

use chrono::{TimeDelta, Utc};
use oxo_core::LogEntry;

/// A label+value pair whose error rate changed significantly.
#[derive(Debug, Clone)]
pub struct LabelChange {
    /// The label name (e.g. `"service"`).
    pub label: String,
    /// The label value (e.g. `"payments"`).
    pub value: String,
    /// Error rate during the baseline window (0.0–1.0).
    pub baseline_error_rate: f64,
    /// Error rate during the comparison (recent) window (0.0–1.0).
    pub current_error_rate: f64,
    /// Ratio of current to baseline error rate.
    pub change_factor: f64,
}

/// Result of a correlation analysis.
#[derive(Debug, Clone)]
pub struct CorrelationResult {
    /// Label changes ranked by `change_factor` descending.
    pub top_changes: Vec<LabelChange>,
}

/// Engine that analyzes log entries to find label values correlated with
/// error rate increases.
pub struct CorrelationEngine {
    /// How many minutes back from the split point for the baseline window.
    baseline_minutes: u64,
    /// How many recent minutes to use as the comparison window.
    comparison_minutes: u64,
}

impl CorrelationEngine {
    /// Create a new correlation engine.
    ///
    /// - `baseline_minutes`: length of the baseline window in minutes.
    /// - `comparison_minutes`: length of the recent comparison window in
    ///   minutes.
    pub fn new(baseline_minutes: u64, comparison_minutes: u64) -> Self {
        Self {
            baseline_minutes,
            comparison_minutes,
        }
    }

    /// Analyze a slice of log entries and return correlated label changes.
    ///
    /// Entries are split into a baseline window and a comparison window based
    /// on the most recent entry's timestamp. For each label+value combination,
    /// the error rate (fraction of entries with level "error" or "fatal") is
    /// computed in both windows, and the results are ranked by change factor.
    pub fn analyze(&self, entries: &[LogEntry]) -> CorrelationResult {
        if entries.is_empty() {
            return CorrelationResult {
                top_changes: Vec::new(),
            };
        }

        // Find the time boundary.
        let max_ts = entries
            .iter()
            .map(|e| e.timestamp)
            .max()
            .unwrap_or_else(Utc::now);

        let comparison_start =
            max_ts - TimeDelta::minutes(self.comparison_minutes as i64);
        let baseline_end = comparison_start;
        let baseline_start =
            baseline_end - TimeDelta::minutes(self.baseline_minutes as i64);

        // Accumulate counts per label+value in each window.
        let mut baseline_counts: HashMap<(String, String), (usize, usize)> = HashMap::new();
        let mut comparison_counts: HashMap<(String, String), (usize, usize)> = HashMap::new();

        for entry in entries {
            let is_error = is_error_entry(entry);
            let ts = entry.timestamp;

            let target = if ts >= comparison_start && ts <= max_ts {
                Some(&mut comparison_counts)
            } else if ts >= baseline_start && ts < baseline_end {
                Some(&mut baseline_counts)
            } else {
                None
            };

            if let Some(counts) = target {
                for (label, value) in &entry.labels {
                    let key = (label.clone(), value.clone());
                    let entry = counts.entry(key).or_insert((0, 0));
                    entry.0 += 1; // total
                    if is_error {
                        entry.1 += 1; // errors
                    }
                }
            }
        }

        // Compute error rates and change factors.
        let mut changes: Vec<LabelChange> = Vec::new();

        // Collect all label+value keys from both windows.
        let all_keys: Vec<(String, String)> = comparison_counts
            .keys()
            .chain(baseline_counts.keys())
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for (label, value) in all_keys {
            let key = (label.clone(), value.clone());

            let baseline = baseline_counts.get(&key).copied().unwrap_or((0, 0));
            let comparison = comparison_counts.get(&key).copied().unwrap_or((0, 0));

            let baseline_rate = if baseline.0 > 0 {
                baseline.1 as f64 / baseline.0 as f64
            } else {
                0.0
            };

            let current_rate = if comparison.0 > 0 {
                comparison.1 as f64 / comparison.0 as f64
            } else {
                0.0
            };

            // Only include entries where there are errors in the comparison window.
            if comparison.1 == 0 {
                continue;
            }

            let change_factor = if baseline_rate > 0.0 {
                current_rate / baseline_rate
            } else if current_rate > 0.0 {
                // Went from zero errors to some errors — treat as a large change.
                f64::MAX
            } else {
                1.0
            };

            changes.push(LabelChange {
                label,
                value,
                baseline_error_rate: baseline_rate,
                current_error_rate: current_rate,
                change_factor,
            });
        }

        // Sort by change_factor descending, take top 10.
        changes.sort_by(|a, b| {
            b.change_factor
                .partial_cmp(&a.change_factor)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        changes.truncate(10);

        CorrelationResult {
            top_changes: changes,
        }
    }
}

/// Check whether a log entry represents an error.
///
/// Looks for a `level` label with value `"error"` or `"fatal"` (case-insensitive).
fn is_error_entry(entry: &LogEntry) -> bool {
    if let Some(level) = entry.labels.get("level") {
        let lower = level.to_ascii_lowercase();
        lower == "error" || lower == "fatal"
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use std::collections::BTreeMap;

    fn make_entry(
        ts: DateTime<Utc>,
        level: &str,
        service: &str,
    ) -> LogEntry {
        let mut labels = BTreeMap::new();
        labels.insert("level".to_string(), level.to_string());
        labels.insert("service".to_string(), service.to_string());
        LogEntry {
            timestamp: ts,
            labels,
            line: format!("[{}] {} log line", level, service),
            raw: None,
        }
    }

    #[test]
    fn detects_service_with_increasing_error_rate() {
        let now = Utc::now();
        let engine = CorrelationEngine::new(30, 5);

        let mut entries = Vec::new();

        // Baseline window: 30–5 minutes ago. Service-A is healthy.
        for i in 6..30 {
            let ts = now - TimeDelta::minutes(i);
            entries.push(make_entry(ts, "info", "service-a"));
            entries.push(make_entry(ts, "info", "service-b"));
        }

        // Comparison window: last 5 minutes. Service-A now errors.
        for i in 0..5 {
            let ts = now - TimeDelta::minutes(i);
            entries.push(make_entry(ts, "error", "service-a"));
            entries.push(make_entry(ts, "info", "service-b"));
        }

        let result = engine.analyze(&entries);
        assert!(
            !result.top_changes.is_empty(),
            "expected at least one correlated label change"
        );

        // service-a should appear as a top change.
        let service_a_change = result
            .top_changes
            .iter()
            .find(|c| c.label == "service" && c.value == "service-a");
        assert!(
            service_a_change.is_some(),
            "service-a should appear in correlated changes"
        );
        let change = service_a_change.unwrap();
        assert!(
            change.current_error_rate > change.baseline_error_rate,
            "service-a error rate should have increased"
        );
    }

    #[test]
    fn empty_entries_produce_empty_result() {
        let engine = CorrelationEngine::new(30, 5);
        let result = engine.analyze(&[]);
        assert!(result.top_changes.is_empty());
    }
}
