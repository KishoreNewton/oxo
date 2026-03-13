//! Generic query-related types shared across backends.
//!
//! These types allow the TUI to express time ranges and pagination without
//! knowing the specifics of any backend's query language.

use chrono::{DateTime, Utc};

/// A time range for log queries.
///
/// Both bounds are inclusive. Backends translate this into their native
/// range parameters (e.g. Loki's `start` / `end` query params).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    /// Start of the range (inclusive).
    pub start: DateTime<Utc>,
    /// End of the range (inclusive).
    pub end: DateTime<Utc>,
}

impl TimeRange {
    /// Create a new time range.
    ///
    /// # Panics
    ///
    /// Panics if `start` is after `end`.
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        assert!(start <= end, "start must be <= end");
        Self { start, end }
    }

    /// Create a range covering the last `duration` up to now.
    pub fn last(duration: chrono::Duration) -> Self {
        let end = Utc::now();
        let start = end - duration;
        Self { start, end }
    }

    /// Duration of this time range.
    pub fn duration(&self) -> chrono::Duration {
        self.end - self.start
    }
}
