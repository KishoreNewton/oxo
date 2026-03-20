//! Per-rule mutable state: cooldowns and rate windows.
//!
//! Each alert rule gets its own [`RuleState`] that tracks when it last fired,
//! how many times it has fired, and (for rate-based rules) a sliding window
//! of recent event timestamps.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Sliding-window counter that tracks events within a fixed duration.
pub struct RateWindow {
    /// The window duration.
    window: Duration,
    /// Timestamps of events within the window, oldest first.
    entries: VecDeque<Instant>,
}

impl RateWindow {
    /// Create a new rate window with the given duration in seconds.
    pub fn new(window_seconds: u64) -> Self {
        Self {
            window: Duration::from_secs(window_seconds),
            entries: VecDeque::new(),
        }
    }

    /// Record a new event at the current instant, evicting stale entries.
    pub fn record(&mut self) {
        let now = Instant::now();
        self.evict(now);
        self.entries.push_back(now);
    }

    /// Return the number of events currently within the window, evicting stale
    /// entries first.
    pub fn count(&mut self) -> u64 {
        let now = Instant::now();
        self.evict(now);
        self.entries.len() as u64
    }

    /// Remove entries that have fallen outside the window.
    fn evict(&mut self, now: Instant) {
        while let Some(&front) = self.entries.front() {
            if now.duration_since(front) > self.window {
                self.entries.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Mutable state held for each alert rule.
pub struct RuleState {
    /// When this rule last fired (if ever).
    pub last_fired: Option<Instant>,
    /// Total number of times this rule has fired.
    pub fire_count: u64,
    /// Sliding window for rate-threshold rules.
    pub rate_window: RateWindow,
}

impl RuleState {
    /// Create fresh state for a rule. `window_seconds` is used to initialise
    /// the rate window (pass 0 if the rule is not rate-based).
    pub fn new(window_seconds: u64) -> Self {
        Self {
            last_fired: None,
            fire_count: 0,
            rate_window: RateWindow::new(window_seconds),
        }
    }

    /// Check whether the cooldown period has elapsed since the last firing.
    ///
    /// Returns `true` if the rule has never fired or if the cooldown has
    /// expired.
    pub fn can_fire(&self, cooldown: Duration) -> bool {
        match self.last_fired {
            None => true,
            Some(last) => last.elapsed() >= cooldown,
        }
    }

    /// Mark this rule as having just fired.
    pub fn mark_fired(&mut self) {
        self.last_fired = Some(Instant::now());
        self.fire_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn rate_window_records_and_counts() {
        let mut w = RateWindow::new(10);
        assert_eq!(w.count(), 0);
        w.record();
        w.record();
        w.record();
        assert_eq!(w.count(), 3);
    }

    #[test]
    fn rate_window_evicts_old_entries() {
        // Use a very short window so entries expire quickly.
        let mut w = RateWindow::new(0); // 0-second window
        w.entries.push_back(Instant::now() - Duration::from_secs(1));
        assert_eq!(
            w.count(),
            0,
            "1-second-old entry should be evicted from a 0s window"
        );
    }

    #[test]
    fn cooldown_enforcement() {
        let mut state = RuleState::new(0);
        let cooldown = Duration::from_millis(50);

        // First fire should always be allowed.
        assert!(state.can_fire(cooldown));
        state.mark_fired();
        assert_eq!(state.fire_count, 1);

        // Immediately after firing, cooldown has not elapsed.
        assert!(!state.can_fire(cooldown));

        // Wait for cooldown to elapse.
        thread::sleep(Duration::from_millis(60));
        assert!(state.can_fire(cooldown));
    }

    #[test]
    fn never_fired_can_always_fire() {
        let state = RuleState::new(0);
        assert!(state.can_fire(Duration::from_secs(3600)));
    }
}
