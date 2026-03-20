//! Anomaly detection for log streams.
//!
//! Two complementary detectors:
//!
//! - [`VolumeAnomalyDetector`] — detects sudden spikes in log volume using
//!   Z-score analysis against a sliding window of recent tick counts.
//! - [`NewPatternDetector`] — detects never-before-seen log patterns after
//!   an initial learning phase.

use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};

use chrono::{DateTime, Utc};

// ---------------------------------------------------------------------------
// Volume anomaly detection
// ---------------------------------------------------------------------------

/// Detects volume spikes using Z-score analysis.
///
/// Maintains a sliding window of per-tick entry counts. When a new tick's
/// count exceeds `mean + z_threshold * stddev`, an anomaly is reported.
pub struct VolumeAnomalyDetector {
    /// Sliding window of recent tick counts.
    window: VecDeque<f64>,
    /// Maximum size of the sliding window.
    window_size: usize,
    /// Z-score threshold for triggering an anomaly.
    z_threshold: f64,
}

/// A detected volume spike.
#[derive(Debug, Clone)]
pub struct VolumeAnomaly {
    /// When the anomaly was detected.
    pub timestamp: DateTime<Utc>,
    /// The expected (mean) rate from the sliding window.
    pub expected_rate: f64,
    /// The actual rate that triggered the anomaly.
    pub actual_rate: f64,
    /// The Z-score of the actual rate.
    pub z_score: f64,
}

impl VolumeAnomalyDetector {
    /// Create a new volume anomaly detector.
    ///
    /// - `window_size`: number of ticks to retain for mean/stddev computation.
    /// - `z_threshold`: how many standard deviations above the mean triggers
    ///   an anomaly (typically 2.0–3.0).
    pub fn new(window_size: usize, z_threshold: f64) -> Self {
        Self {
            window: VecDeque::with_capacity(window_size),
            window_size,
            z_threshold,
        }
    }

    /// Record a tick's entry count and check for a volume anomaly.
    ///
    /// Returns `Some(VolumeAnomaly)` if the count is anomalously high.
    pub fn record_tick(&mut self, count: f64) -> Option<VolumeAnomaly> {
        let anomaly = if self.window.len() >= 2 {
            let mean = self.mean();
            let stddev = self.stddev(mean);

            if stddev > 0.0 {
                let z_score = (count - mean) / stddev;
                if z_score > self.z_threshold {
                    Some(VolumeAnomaly {
                        timestamp: Utc::now(),
                        expected_rate: mean,
                        actual_rate: count,
                        z_score,
                    })
                } else {
                    None
                }
            } else {
                // Zero stddev — all values are the same. A different value
                // is technically infinite Z but we only flag if count is
                // significantly above mean (or any spike from a zero baseline).
                if count > 0.0 && (mean == 0.0 || count > mean * 2.0) {
                    Some(VolumeAnomaly {
                        timestamp: Utc::now(),
                        expected_rate: mean,
                        actual_rate: count,
                        z_score: f64::INFINITY,
                    })
                } else {
                    None
                }
            }
        } else {
            None
        };

        // Push count into the window.
        self.window.push_back(count);
        while self.window.len() > self.window_size {
            self.window.pop_front();
        }

        anomaly
    }

    fn mean(&self) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.window.iter().sum();
        sum / self.window.len() as f64
    }

    fn stddev(&self, mean: f64) -> f64 {
        if self.window.len() < 2 {
            return 0.0;
        }
        let variance =
            self.window.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / self.window.len() as f64;
        variance.sqrt()
    }
}

// ---------------------------------------------------------------------------
// New pattern detection
// ---------------------------------------------------------------------------

/// Detects never-before-seen log patterns after a learning phase.
///
/// During the learning phase, all observed pattern templates are recorded.
/// After `learning_threshold` entries have been seen, the detector transitions
/// to detection mode: any new pattern not seen during learning is flagged.
pub struct NewPatternDetector {
    /// Hashes of known pattern templates.
    known_patterns: HashSet<u64>,
    /// Whether we are still in the learning phase.
    learning: bool,
    /// Number of entries to observe before ending the learning phase.
    learning_threshold: usize,
    /// Total entries seen so far.
    entries_seen: usize,
}

/// A newly detected, previously unseen log pattern.
#[derive(Debug, Clone)]
pub struct NewPatternEvent {
    /// The template of the new pattern.
    pub template: String,
    /// When this pattern was first observed.
    pub first_seen: DateTime<Utc>,
    /// An example log line that produced this pattern.
    pub example: String,
}

impl NewPatternDetector {
    /// Create a new pattern detector.
    ///
    /// - `learning_threshold`: how many entries to observe during the learning
    ///   phase before switching to detection mode.
    pub fn new(learning_threshold: usize) -> Self {
        Self {
            known_patterns: HashSet::new(),
            learning: true,
            learning_threshold,
            entries_seen: 0,
        }
    }

    /// Check a clustered pattern template.
    ///
    /// During learning: records the template hash and returns `None`.
    /// After learning: returns `Some(NewPatternEvent)` if this is a
    /// never-before-seen template.
    pub fn check(
        &mut self,
        template: &str,
        example: &str,
        timestamp: DateTime<Utc>,
    ) -> Option<NewPatternEvent> {
        let hash = self.hash_template(template);
        self.entries_seen += 1;

        if self.learning {
            self.known_patterns.insert(hash);
            if self.entries_seen >= self.learning_threshold {
                self.learning = false;
            }
            return None;
        }

        if self.known_patterns.insert(hash) {
            // Newly inserted — this pattern was not seen during learning.
            Some(NewPatternEvent {
                template: template.to_string(),
                first_seen: timestamp,
                example: example.to_string(),
            })
        } else {
            None
        }
    }

    /// Whether the detector is still in the learning phase.
    pub fn is_learning(&self) -> bool {
        self.learning
    }

    /// Hash a template string using the standard library's hasher.
    fn hash_template(&self, template: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        template.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_spike_detected() {
        let mut d = VolumeAnomalyDetector::new(10, 2.0);

        // Feed a steady baseline.
        for _ in 0..10 {
            assert!(d.record_tick(100.0).is_none());
        }

        // Inject a spike — should be detected.
        let anomaly = d.record_tick(500.0);
        assert!(anomaly.is_some(), "expected a volume anomaly");
        let a = anomaly.unwrap();
        assert!(a.actual_rate > a.expected_rate);
        assert!(a.z_score > 2.0);
    }

    #[test]
    fn no_anomaly_on_steady_volume() {
        let mut d = VolumeAnomalyDetector::new(10, 3.0);

        for _ in 0..20 {
            assert!(d.record_tick(100.0).is_none());
        }
    }

    #[test]
    fn learning_phase_records_patterns() {
        let mut d = NewPatternDetector::new(5);
        let ts = Utc::now();

        // During learning, all patterns should return None.
        for i in 0..5 {
            let tpl = format!("pattern-{}", i);
            assert!(d.check(&tpl, "example", ts).is_none());
        }

        assert!(!d.is_learning(), "should have exited learning phase");
    }

    #[test]
    fn new_pattern_detected_after_learning() {
        let mut d = NewPatternDetector::new(3);
        let ts = Utc::now();

        // Learning phase.
        d.check("GET /api/{*}", "GET /api/123", ts);
        d.check("POST /api/{*}", "POST /api/456", ts);
        d.check("DELETE /api/{*}", "DELETE /api/789", ts);

        assert!(!d.is_learning());

        // Known pattern — should not trigger.
        assert!(d.check("GET /api/{*}", "GET /api/111", ts).is_none());

        // New pattern — should trigger.
        let event = d.check(
            "FATAL database timeout {*}",
            "FATAL database timeout 30s",
            ts,
        );
        assert!(event.is_some());
        let e = event.unwrap();
        assert_eq!(e.template, "FATAL database timeout {*}");
    }

    #[test]
    fn same_new_pattern_only_flagged_once() {
        let mut d = NewPatternDetector::new(1);
        let ts = Utc::now();

        // Learning.
        d.check("known-pattern", "example", ts);

        // First time seeing "new-pattern" => flagged.
        assert!(d.check("new-pattern", "example", ts).is_some());

        // Second time => already known, not flagged.
        assert!(d.check("new-pattern", "example", ts).is_none());
    }
}
