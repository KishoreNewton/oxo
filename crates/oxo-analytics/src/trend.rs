//! Trend analysis via linear regression.
//!
//! Buckets log entries by time, computes the error ratio in each bucket, and
//! performs ordinary least-squares linear regression to determine whether the
//! error rate is increasing, decreasing, or stable.

use chrono::{DateTime, TimeDelta, Utc};
use oxo_core::LogEntry;

/// Result of a trend analysis.
#[derive(Debug, Clone)]
pub struct TrendResult {
    /// Slope of the regression line. Positive = error rate increasing.
    pub slope: f64,
    /// R-squared (coefficient of determination). 1.0 = perfect fit.
    pub r_squared: f64,
    /// Human-readable description of the trend.
    pub description: String,
    /// The (timestamp, error_ratio) data points used for the regression.
    pub data_points: Vec<(DateTime<Utc>, f64)>,
}

/// Trend analyzer that performs linear regression on error rate over time.
pub struct TrendAnalyzer {
    /// Number of time buckets to divide the data into.
    bucket_count: usize,
    /// Width of each bucket in seconds.
    bucket_seconds: u64,
}

impl TrendAnalyzer {
    /// Create a new trend analyzer.
    ///
    /// - `bucket_count`: number of time buckets.
    /// - `bucket_seconds`: width of each bucket in seconds.
    pub fn new(bucket_count: usize, bucket_seconds: u64) -> Self {
        Self {
            bucket_count,
            bucket_seconds,
        }
    }

    /// Analyze the error rate trend from a slice of log entries.
    ///
    /// Returns `None` if there are fewer than 2 data points with entries.
    pub fn analyze(&self, entries: &[LogEntry]) -> Option<TrendResult> {
        if entries.is_empty() {
            return None;
        }

        // Find the time range.
        let max_ts = entries.iter().map(|e| e.timestamp).max()?;
        let total_span = self.bucket_count as i64 * self.bucket_seconds as i64;
        let start_ts = max_ts - TimeDelta::seconds(total_span);

        // Initialize buckets.
        let mut buckets: Vec<(DateTime<Utc>, usize, usize)> = (0..self.bucket_count)
            .map(|i| {
                let bucket_start =
                    start_ts + TimeDelta::seconds(i as i64 * self.bucket_seconds as i64);
                (bucket_start, 0_usize, 0_usize) // (timestamp, total, errors)
            })
            .collect();

        // Assign entries to buckets.
        for entry in entries {
            let offset = entry.timestamp.signed_duration_since(start_ts);
            let offset_secs = offset.num_seconds();
            if offset_secs < 0 {
                continue;
            }

            let bucket_idx = (offset_secs / self.bucket_seconds as i64) as usize;
            if bucket_idx >= self.bucket_count {
                continue;
            }

            buckets[bucket_idx].1 += 1; // total
            if is_error_entry(entry) {
                buckets[bucket_idx].2 += 1; // errors
            }
        }

        // Compute error ratio per bucket, filtering out empty buckets.
        let data_points: Vec<(DateTime<Utc>, f64)> = buckets
            .iter()
            .filter(|(_, total, _)| *total > 0)
            .map(|(ts, total, errors)| (*ts, *errors as f64 / *total as f64))
            .collect();

        if data_points.len() < 2 {
            return None;
        }

        // Perform least-squares linear regression.
        // Use bucket indices (0, 1, 2, ...) as x-values for numerical stability.
        let n = data_points.len() as f64;
        let x_values: Vec<f64> = (0..data_points.len()).map(|i| i as f64).collect();
        let y_values: Vec<f64> = data_points.iter().map(|(_, ratio)| *ratio).collect();

        let sum_x: f64 = x_values.iter().sum();
        let sum_y: f64 = y_values.iter().sum();
        let sum_xy: f64 = x_values
            .iter()
            .zip(y_values.iter())
            .map(|(x, y)| x * y)
            .sum();
        let sum_x2: f64 = x_values.iter().map(|x| x * x).sum();

        let denominator = n * sum_x2 - sum_x * sum_x;
        if denominator.abs() < f64::EPSILON {
            return None;
        }

        let slope = (n * sum_xy - sum_x * sum_y) / denominator;
        let intercept = (sum_y - slope * sum_x) / n;

        // Compute R-squared.
        let y_mean = sum_y / n;
        let ss_tot: f64 = y_values.iter().map(|y| (y - y_mean).powi(2)).sum();
        let ss_res: f64 = x_values
            .iter()
            .zip(y_values.iter())
            .map(|(x, y)| {
                let predicted = slope * x + intercept;
                (y - predicted).powi(2)
            })
            .sum();

        let r_squared = if ss_tot > 0.0 {
            1.0 - (ss_res / ss_tot)
        } else {
            0.0
        };

        // Convert slope from "per bucket index" to "per minute" for the
        // description.
        let slope_per_minute = if self.bucket_seconds > 0 {
            slope / (self.bucket_seconds as f64 / 60.0)
        } else {
            slope
        };

        let description = if slope_per_minute.abs() < 0.001 {
            "Error rate stable".to_string()
        } else if slope_per_minute > 0.0 {
            format!(
                "Error rate increasing: +{:.1}%/min (R\u{00b2}={:.2})",
                slope_per_minute * 100.0,
                r_squared
            )
        } else {
            format!(
                "Error rate decreasing: {:.1}%/min (R\u{00b2}={:.2})",
                slope_per_minute * 100.0,
                r_squared
            )
        };

        Some(TrendResult {
            slope,
            r_squared,
            description,
            data_points,
        })
    }
}

/// Check whether a log entry represents an error.
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
    use std::collections::BTreeMap;

    fn make_entry(ts: DateTime<Utc>, level: &str) -> LogEntry {
        let mut labels = BTreeMap::new();
        labels.insert("level".to_string(), level.to_string());
        LogEntry {
            timestamp: ts,
            labels,
            line: format!("[{}] test line", level),
            raw: None,
        }
    }

    #[test]
    fn increasing_error_rate_gives_positive_slope() {
        let now = Utc::now();
        let analyzer = TrendAnalyzer::new(10, 60);

        let mut entries = Vec::new();

        // Earlier buckets: mostly info.
        for i in (5..10).rev() {
            let ts = now - TimeDelta::seconds(i * 60);
            for _ in 0..10 {
                entries.push(make_entry(ts, "info"));
            }
            // 1 error per bucket.
            entries.push(make_entry(ts, "error"));
        }

        // Later buckets: increasing errors.
        for i in (0..5).rev() {
            let ts = now - TimeDelta::seconds(i * 60);
            for _ in 0..5 {
                entries.push(make_entry(ts, "info"));
            }
            // Many errors per bucket.
            for _ in 0..5 {
                entries.push(make_entry(ts, "error"));
            }
        }

        let result = analyzer.analyze(&entries);
        assert!(result.is_some(), "expected a trend result");

        let trend = result.unwrap();
        assert!(
            trend.slope > 0.0,
            "expected positive slope, got {}",
            trend.slope
        );
        assert!(
            trend.description.contains("increasing"),
            "expected 'increasing' in description: {}",
            trend.description
        );
    }

    #[test]
    fn stable_error_rate_gives_near_zero_slope() {
        let now = Utc::now();
        let analyzer = TrendAnalyzer::new(10, 60);

        let mut entries = Vec::new();

        // All buckets: same error rate (~10%).
        for i in (0..10).rev() {
            let ts = now - TimeDelta::seconds(i * 60);
            for _ in 0..9 {
                entries.push(make_entry(ts, "info"));
            }
            entries.push(make_entry(ts, "error"));
        }

        let result = analyzer.analyze(&entries);
        assert!(result.is_some());

        let trend = result.unwrap();
        assert!(
            trend.slope.abs() < 0.1,
            "expected near-zero slope, got {}",
            trend.slope
        );
    }

    #[test]
    fn empty_entries_return_none() {
        let analyzer = TrendAnalyzer::new(10, 60);
        assert!(analyzer.analyze(&[]).is_none());
    }
}
