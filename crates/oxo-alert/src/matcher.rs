//! Rule matching logic.
//!
//! [`CompiledRule`] pre-compiles alert conditions (regex patterns, level
//! lookups) so evaluation at runtime is as cheap as possible. The
//! [`matches`](CompiledRule::matches) method checks a [`LogEntry`] against
//! the compiled condition and updates the rule's [`RuleState`].

use std::collections::HashMap;

use oxo_core::LogEntry;
use regex::Regex;

use crate::config::AlertCondition;
use crate::state::RuleState;

/// A compiled, ready-to-evaluate alert rule.
pub struct CompiledRule {
    /// Human-readable name (mirrors [`AlertRule::name`](crate::config::AlertRule::name)).
    pub name: String,
    /// The compiled condition.
    pub condition: CompiledCondition,
    /// Label filters that must all match before the condition is evaluated.
    pub label_filters: HashMap<String, String>,
}

/// A condition that has been compiled for fast evaluation.
pub enum CompiledCondition {
    /// Matches when the log line matches the regex.
    PatternMatch(Regex),
    /// Matches when the number of events in the sliding window meets or
    /// exceeds `count` within `window_seconds`.
    RateThreshold { count: u64, window_seconds: u64 },
    /// Matches when the log entry's level is one of the listed values.
    /// The vector contains all levels at or above the configured threshold.
    LevelThreshold(Vec<String>),
}

/// Ordered severity levels from least to most severe.
const LEVEL_ORDER: &[&str] = &["trace", "debug", "info", "warn", "error", "fatal"];

/// Return all levels at or above `threshold` (inclusive).
fn levels_at_or_above(threshold: &str) -> Vec<String> {
    let lower = threshold.to_lowercase();
    let idx = LEVEL_ORDER
        .iter()
        .position(|&l| l == lower)
        .unwrap_or(LEVEL_ORDER.len());
    LEVEL_ORDER[idx..].iter().map(|s| s.to_string()).collect()
}

impl CompiledRule {
    /// Compile an alert rule into a [`CompiledRule`].
    ///
    /// Returns an error only if a `PatternMatch` condition contains an invalid
    /// regex.
    pub fn compile(
        name: String,
        condition: &AlertCondition,
        labels: HashMap<String, String>,
    ) -> Result<Self, regex::Error> {
        let compiled = match condition {
            AlertCondition::PatternMatch { pattern } => {
                CompiledCondition::PatternMatch(Regex::new(pattern)?)
            }
            AlertCondition::RateThreshold {
                count,
                window_seconds,
            } => CompiledCondition::RateThreshold {
                count: *count,
                window_seconds: *window_seconds,
            },
            AlertCondition::LevelThreshold { level } => {
                CompiledCondition::LevelThreshold(levels_at_or_above(level))
            }
        };
        Ok(Self {
            name,
            condition: compiled,
            label_filters: labels,
        })
    }

    /// Evaluate whether `entry` satisfies this rule's condition.
    ///
    /// For rate-threshold rules, this also records the event in `state`'s rate
    /// window so subsequent calls reflect the updated count.
    ///
    /// Label filters are checked first — if any required label is missing or
    /// has the wrong value, the method returns `false` without evaluating the
    /// condition.
    pub fn matches(&self, entry: &LogEntry, state: &mut RuleState) -> bool {
        // 1. Check label filters.
        for (key, expected) in &self.label_filters {
            match entry.labels.get(key) {
                Some(actual) if actual == expected => {}
                _ => return false,
            }
        }

        // 2. Evaluate the condition.
        match &self.condition {
            CompiledCondition::PatternMatch(re) => re.is_match(&entry.line),
            CompiledCondition::RateThreshold { count, .. } => {
                // Always record the event so the window is accurate.
                state.rate_window.record();
                state.rate_window.count() >= *count
            }
            CompiledCondition::LevelThreshold(levels) => {
                // Look for a "level" label on the entry (case-insensitive).
                let entry_level = entry
                    .labels
                    .get("level")
                    .or_else(|| entry.labels.get("severity"))
                    .map(|s| s.to_lowercase());
                match entry_level {
                    Some(ref l) => levels.iter().any(|t| t == l),
                    None => false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Utc;
    use oxo_core::LogEntry;

    use super::*;
    use crate::config::AlertCondition;
    use crate::state::RuleState;

    fn make_entry(line: &str, labels: &[(&str, &str)]) -> LogEntry {
        let mut lmap = BTreeMap::new();
        for &(k, v) in labels {
            lmap.insert(k.to_string(), v.to_string());
        }
        LogEntry {
            timestamp: Utc::now(),
            labels: lmap,
            line: line.to_string(),
            raw: None,
        }
    }

    #[test]
    fn pattern_match_positive() {
        let cond = AlertCondition::PatternMatch {
            pattern: "ERROR|FATAL".to_string(),
        };
        let rule =
            CompiledRule::compile("test".into(), &cond, HashMap::new()).expect("valid regex");
        let entry = make_entry("2024-01-01 ERROR something broke", &[]);
        let mut state = RuleState::new(0);
        assert!(rule.matches(&entry, &mut state));
    }

    #[test]
    fn pattern_match_negative() {
        let cond = AlertCondition::PatternMatch {
            pattern: "ERROR|FATAL".to_string(),
        };
        let rule =
            CompiledRule::compile("test".into(), &cond, HashMap::new()).expect("valid regex");
        let entry = make_entry("2024-01-01 INFO all good", &[]);
        let mut state = RuleState::new(0);
        assert!(!rule.matches(&entry, &mut state));
    }

    #[test]
    fn invalid_regex_returns_error() {
        let cond = AlertCondition::PatternMatch {
            pattern: "[invalid".to_string(),
        };
        assert!(CompiledRule::compile("bad".into(), &cond, HashMap::new()).is_err());
    }

    #[test]
    fn level_threshold_warn() {
        let cond = AlertCondition::LevelThreshold {
            level: "warn".to_string(),
        };
        let rule =
            CompiledRule::compile("test".into(), &cond, HashMap::new()).expect("valid");
        let mut state = RuleState::new(0);

        // "error" is at or above "warn".
        let entry = make_entry("something", &[("level", "error")]);
        assert!(rule.matches(&entry, &mut state));

        // "info" is below "warn".
        let entry = make_entry("something", &[("level", "info")]);
        assert!(!rule.matches(&entry, &mut state));

        // "warn" itself should match.
        let entry = make_entry("something", &[("level", "warn")]);
        assert!(rule.matches(&entry, &mut state));

        // "fatal" is above "warn".
        let entry = make_entry("something", &[("level", "fatal")]);
        assert!(rule.matches(&entry, &mut state));
    }

    #[test]
    fn level_threshold_error() {
        let cond = AlertCondition::LevelThreshold {
            level: "error".to_string(),
        };
        let rule =
            CompiledRule::compile("test".into(), &cond, HashMap::new()).expect("valid");
        let mut state = RuleState::new(0);

        let entry = make_entry("x", &[("level", "error")]);
        assert!(rule.matches(&entry, &mut state));

        let entry = make_entry("x", &[("level", "fatal")]);
        assert!(rule.matches(&entry, &mut state));

        let entry = make_entry("x", &[("level", "warn")]);
        assert!(!rule.matches(&entry, &mut state));
    }

    #[test]
    fn level_threshold_fatal() {
        let cond = AlertCondition::LevelThreshold {
            level: "fatal".to_string(),
        };
        let rule =
            CompiledRule::compile("test".into(), &cond, HashMap::new()).expect("valid");
        let mut state = RuleState::new(0);

        let entry = make_entry("x", &[("level", "fatal")]);
        assert!(rule.matches(&entry, &mut state));

        let entry = make_entry("x", &[("level", "error")]);
        assert!(!rule.matches(&entry, &mut state));
    }

    #[test]
    fn level_threshold_uses_severity_label() {
        let cond = AlertCondition::LevelThreshold {
            level: "error".to_string(),
        };
        let rule =
            CompiledRule::compile("test".into(), &cond, HashMap::new()).expect("valid");
        let mut state = RuleState::new(0);

        let entry = make_entry("x", &[("severity", "fatal")]);
        assert!(rule.matches(&entry, &mut state));
    }

    #[test]
    fn level_threshold_no_level_label() {
        let cond = AlertCondition::LevelThreshold {
            level: "error".to_string(),
        };
        let rule =
            CompiledRule::compile("test".into(), &cond, HashMap::new()).expect("valid");
        let mut state = RuleState::new(0);

        let entry = make_entry("ERROR in the line but no label", &[]);
        assert!(!rule.matches(&entry, &mut state));
    }

    #[test]
    fn rate_threshold_counts_events() {
        let cond = AlertCondition::RateThreshold {
            count: 3,
            window_seconds: 60,
        };
        let rule =
            CompiledRule::compile("test".into(), &cond, HashMap::new()).expect("valid");
        let mut state = RuleState::new(60);

        let entry = make_entry("log line", &[]);

        // First two should not trigger.
        assert!(!rule.matches(&entry, &mut state));
        assert!(!rule.matches(&entry, &mut state));
        // Third should trigger.
        assert!(rule.matches(&entry, &mut state));
        // Fourth also triggers (still above threshold).
        assert!(rule.matches(&entry, &mut state));
    }

    #[test]
    fn label_filter_blocks_non_matching() {
        let cond = AlertCondition::PatternMatch {
            pattern: ".*".to_string(),
        };
        let mut labels = HashMap::new();
        labels.insert("env".to_string(), "production".to_string());
        let rule =
            CompiledRule::compile("test".into(), &cond, labels).expect("valid");
        let mut state = RuleState::new(0);

        // Entry without the required label should not match.
        let entry = make_entry("anything", &[]);
        assert!(!rule.matches(&entry, &mut state));

        // Entry with wrong value should not match.
        let entry = make_entry("anything", &[("env", "staging")]);
        assert!(!rule.matches(&entry, &mut state));

        // Entry with correct label should match.
        let entry = make_entry("anything", &[("env", "production")]);
        assert!(rule.matches(&entry, &mut state));
    }
}
