//! Drain-inspired log pattern clustering.
//!
//! The Drain algorithm automatically discovers log templates by tokenizing
//! log lines and grouping them by token count and content similarity. Variable
//! parts (timestamps, IDs, durations, etc.) are replaced with `{*}` wildcards,
//! producing human-readable pattern templates.
//!
//! # Algorithm
//!
//! 1. Tokenize the log line by whitespace and structural delimiters.
//! 2. Look up existing patterns with the same token count.
//! 3. Find the most similar pattern (ratio of matching tokens).
//! 4. If similarity exceeds the threshold, merge — replacing differing tokens
//!    with `{*}`.
//! 5. Otherwise, create a new pattern from the line's tokens.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Utc};

/// Maximum number of recent timestamps retained per pattern.
const MAX_RECENT_TIMESTAMPS: usize = 100;

/// A discovered log pattern template.
#[derive(Debug, Clone)]
pub struct LogPattern {
    /// Template with variables replaced, e.g. `"GET /api/v1/users/{*} {*}ms"`.
    pub template: String,

    /// How many log entries matched this pattern.
    pub count: usize,

    /// Timestamps of recent matches (capped at [`MAX_RECENT_TIMESTAMPS`]).
    pub recent_timestamps: VecDeque<DateTime<Utc>>,

    /// An example log entry that matched this pattern.
    pub example: String,

    /// The tokenized template (internal representation).
    tokens: Vec<String>,
}

/// Drain-inspired log clustering engine.
///
/// Patterns are grouped by token count for O(1) length lookup, then linearly
/// scanned within the group for similarity. This is efficient for typical log
/// workloads where most lines share a small number of templates.
pub struct LogClusterer {
    /// Patterns grouped by token count for fast lookup.
    length_groups: HashMap<usize, Vec<LogPattern>>,

    /// Similarity threshold (0.0–1.0). A log line must match at least this
    /// fraction of tokens to be merged into an existing pattern.
    similarity_threshold: f64,

    /// Maximum total patterns to retain across all groups.
    max_patterns: usize,

    /// Running count of total patterns stored.
    total_pattern_count: usize,

    /// Total entries processed.
    pub total_entries: usize,
}

impl LogClusterer {
    /// Create a new clusterer.
    ///
    /// - `similarity_threshold`: 0.0–1.0 — fraction of tokens that must match.
    ///   Lower values produce broader patterns. Typical: 0.4–0.6.
    /// - `max_patterns`: upper bound on stored patterns. When exceeded, the
    ///   least-used pattern is evicted.
    pub fn new(similarity_threshold: f64, max_patterns: usize) -> Self {
        Self {
            length_groups: HashMap::new(),
            similarity_threshold,
            max_patterns,
            total_pattern_count: 0,
            total_entries: 0,
        }
    }

    /// Ingest a single log line and return the index of the matched pattern
    /// within its length group.
    pub fn ingest(&mut self, line: &str, timestamp: DateTime<Utc>) -> usize {
        self.total_entries += 1;

        let line_tokens = tokenize(line);
        let token_count = line_tokens.len();

        if token_count == 0 {
            return 0;
        }

        let group = self.length_groups.entry(token_count).or_default();

        // Find the best matching pattern.
        let mut best_idx: Option<usize> = None;
        let mut best_sim: f64 = 0.0;

        for (idx, pattern) in group.iter().enumerate() {
            let sim = similarity(&pattern.tokens, &line_tokens);
            if sim > best_sim {
                best_sim = sim;
                best_idx = Some(idx);
            }
        }

        if let Some(idx) = best_idx {
            if best_sim >= self.similarity_threshold {
                // Merge into existing pattern.
                let pattern = &mut group[idx];
                merge_pattern(pattern, &line_tokens, line, timestamp);
                return idx;
            }
        }

        // No sufficient match — create a new pattern.
        self.maybe_evict();

        let tokens: Vec<String> = line_tokens.into_iter().map(|t| t.to_string()).collect();
        let template = tokens.join(" ");

        let mut recent_timestamps = VecDeque::with_capacity(MAX_RECENT_TIMESTAMPS);
        recent_timestamps.push_back(timestamp);

        let new_pattern = LogPattern {
            template,
            count: 1,
            recent_timestamps,
            example: line.to_string(),
            tokens,
        };

        let group = self.length_groups.entry(token_count).or_default();
        group.push(new_pattern);
        self.total_pattern_count += 1;

        group.len() - 1
    }

    /// Return the top N patterns sorted by count (descending).
    pub fn top_patterns(&self, n: usize) -> Vec<&LogPattern> {
        let mut all: Vec<&LogPattern> = self
            .length_groups
            .values()
            .flat_map(|group| group.iter())
            .collect();

        all.sort_by(|a, b| b.count.cmp(&a.count));
        all.truncate(n);
        all
    }

    /// If we have exceeded `max_patterns`, evict the least-used pattern.
    fn maybe_evict(&mut self) {
        if self.total_pattern_count < self.max_patterns {
            return;
        }

        // Find the group and index of the pattern with the lowest count.
        let mut min_count = usize::MAX;
        let mut min_key: Option<usize> = None;
        let mut min_idx: Option<usize> = None;

        for (&key, group) in &self.length_groups {
            for (idx, pattern) in group.iter().enumerate() {
                if pattern.count < min_count {
                    min_count = pattern.count;
                    min_key = Some(key);
                    min_idx = Some(idx);
                }
            }
        }

        if let (Some(key), Some(idx)) = (min_key, min_idx) {
            if let Some(group) = self.length_groups.get_mut(&key) {
                group.swap_remove(idx);
                self.total_pattern_count -= 1;

                // Clean up empty groups.
                if group.is_empty() {
                    self.length_groups.remove(&key);
                }
            }
        }
    }
}

/// Merge a log line into an existing pattern by replacing differing tokens
/// with `{*}` wildcards.
fn merge_pattern(
    pattern: &mut LogPattern,
    line_tokens: &[&str],
    line: &str,
    timestamp: DateTime<Utc>,
) {
    // Update tokens: where they differ, insert wildcard.
    let len = pattern.tokens.len().min(line_tokens.len());
    for i in 0..len {
        if pattern.tokens[i] != "{*}" && pattern.tokens[i] != line_tokens[i] {
            pattern.tokens[i] = "{*}".to_string();
        }
    }

    // Rebuild the template string from tokens.
    pattern.template = pattern.tokens.join(" ");

    pattern.count += 1;
    pattern.example = line.to_string();

    pattern.recent_timestamps.push_back(timestamp);
    while pattern.recent_timestamps.len() > MAX_RECENT_TIMESTAMPS {
        pattern.recent_timestamps.pop_front();
    }
}

/// Tokenize a log line by whitespace and structural delimiters (`=`, `:`).
///
/// This produces tokens suitable for similarity comparison. For example:
/// `"level=info msg:hello world"` → `["level", "info", "msg", "hello", "world"]`
fn tokenize(line: &str) -> Vec<&str> {
    line.split(|c: char| c.is_ascii_whitespace() || c == '=' || c == ':')
        .filter(|s| !s.is_empty())
        .collect()
}

/// Compute similarity between a pattern's tokens and a line's tokens.
///
/// Returns the fraction of token positions that match. The `{*}` wildcard
/// in a pattern always matches any token.
fn similarity(pattern_tokens: &[String], line_tokens: &[&str]) -> f64 {
    let len = pattern_tokens.len();
    if len == 0 {
        return 0.0;
    }

    // Lengths should match (same length group), but guard anyway.
    if len != line_tokens.len() {
        return 0.0;
    }

    let matches = pattern_tokens
        .iter()
        .zip(line_tokens.iter())
        .filter(|(pt, lt)| pt.as_str() == "{*}" || *pt == *lt)
        .count();

    matches as f64 / len as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn similar_http_logs_cluster_together() {
        let mut c = LogClusterer::new(0.4, 100);

        c.ingest("GET /api/v1/users/123 200 45ms", now());
        c.ingest("GET /api/v1/users/456 200 67ms", now());
        c.ingest("GET /api/v1/users/789 200 23ms", now());

        let top = c.top_patterns(10);
        // All three should cluster into one pattern.
        assert_eq!(top.len(), 1, "expected 1 pattern, got {}", top.len());
        assert_eq!(top[0].count, 3);
        assert!(
            top[0].template.contains("{*}"),
            "template should contain wildcards: {}",
            top[0].template
        );
    }

    #[test]
    fn dissimilar_logs_create_separate_patterns() {
        let mut c = LogClusterer::new(0.5, 100);

        c.ingest("GET /api/v1/users/123 200 45ms", now());
        c.ingest("database connection pool exhausted timeout 30s", now());
        c.ingest("kafka consumer group rebalance partition 7", now());

        let top = c.top_patterns(10);
        assert!(
            top.len() >= 3,
            "expected at least 3 patterns, got {}",
            top.len()
        );
    }

    #[test]
    fn wildcards_accumulate_correctly() {
        let mut c = LogClusterer::new(0.4, 100);

        c.ingest("user 123 logged in from 10.0.0.1", now());
        c.ingest("user 456 logged in from 10.0.0.2", now());
        c.ingest("user 789 logged in from 192.168.1.1", now());

        let top = c.top_patterns(1);
        assert_eq!(top.len(), 1);

        let tpl = &top[0].template;
        // The user ID and IP should be wildcarded.
        assert!(
            tpl.contains("{*}"),
            "template should have wildcards: {}",
            tpl
        );
        // The static tokens should remain.
        assert!(tpl.contains("user"), "template missing 'user': {}", tpl);
        assert!(
            tpl.contains("logged"),
            "template missing 'logged': {}",
            tpl
        );
        assert!(tpl.contains("in"), "template missing 'in': {}", tpl);
        assert!(tpl.contains("from"), "template missing 'from': {}", tpl);
    }

    #[test]
    fn tokenize_splits_on_delimiters() {
        let tokens = tokenize("level=info msg:hello world");
        assert_eq!(tokens, vec!["level", "info", "msg", "hello", "world"]);
    }

    #[test]
    fn empty_line_returns_zero() {
        let mut c = LogClusterer::new(0.5, 100);
        let idx = c.ingest("", now());
        assert_eq!(idx, 0);
    }
}
