//! Analytics engine orchestrator.
//!
//! The [`AnalyticsEngine`] ties together all analytics subsystems — clustering,
//! anomaly detection, correlation, trend analysis, and top-N — into a single
//! async task that consumes [`LogEntry`] items from a channel and periodically
//! emits [`AnalyticsSnapshot`]s.

use tokio::sync::mpsc;

use oxo_core::LogEntry;

use crate::anomaly::{NewPatternDetector, NewPatternEvent, VolumeAnomaly, VolumeAnomalyDetector};
use crate::clustering::{LogClusterer, LogPattern};
use crate::correlation::{CorrelationEngine, CorrelationResult};
use crate::topn::{EndpointLatency, TopNAnalyzer};
use crate::trend::{TrendAnalyzer, TrendResult};

/// Snapshot of all analytics results at a point in time.
///
/// Sent periodically to the UI so it can render dashboards.
#[derive(Debug, Clone)]
pub struct AnalyticsSnapshot {
    /// Top log patterns by frequency.
    pub top_patterns: Vec<LogPattern>,
    /// Volume anomalies detected in the last tick.
    pub anomalies: Vec<VolumeAnomaly>,
    /// New log patterns detected since the last snapshot.
    pub new_patterns: Vec<NewPatternEvent>,
    /// Error correlation results (if sufficient data).
    pub correlation: Option<CorrelationResult>,
    /// Error rate trend analysis (if sufficient data).
    pub trend: Option<TrendResult>,
    /// Slowest endpoints by extracted latency.
    pub slowest_endpoints: Vec<EndpointLatency>,
    /// Noisiest sources by entry count.
    pub noisiest_sources: Vec<(String, usize, f64)>,
}

/// Orchestrator that runs all analytics subsystems.
///
/// Feed it [`LogEntry`] items via a channel and it will periodically emit
/// [`AnalyticsSnapshot`]s.
pub struct AnalyticsEngine {
    clusterer: LogClusterer,
    volume_detector: VolumeAnomalyDetector,
    pattern_detector: NewPatternDetector,
    correlation: CorrelationEngine,
    trend: TrendAnalyzer,

    /// Buffered entries for correlation and trend analysis.
    entries: Vec<LogEntry>,
    /// Maximum number of entries to buffer.
    max_entries: usize,

    /// Tick counter (incremented every tick interval).
    tick_count: u64,
    /// Number of entries ingested in the current tick.
    current_tick_entries: u64,

    /// Recent volume anomalies (accumulated between snapshots).
    recent_anomalies: Vec<VolumeAnomaly>,
    /// Recent new-pattern events (accumulated between snapshots).
    recent_new_patterns: Vec<NewPatternEvent>,

    /// Channel for sending snapshots to the UI.
    snapshot_tx: mpsc::UnboundedSender<AnalyticsSnapshot>,
}

impl AnalyticsEngine {
    /// Create a new analytics engine.
    ///
    /// Snapshots are sent via `snapshot_tx` every 5 ticks (5 seconds by
    /// default).
    pub fn new(snapshot_tx: mpsc::UnboundedSender<AnalyticsSnapshot>) -> Self {
        Self {
            clusterer: LogClusterer::new(0.4, 1000),
            volume_detector: VolumeAnomalyDetector::new(60, 3.0),
            pattern_detector: NewPatternDetector::new(1000),
            correlation: CorrelationEngine::new(30, 5),
            trend: TrendAnalyzer::new(30, 60),
            entries: Vec::new(),
            max_entries: 50_000,
            tick_count: 0,
            current_tick_entries: 0,
            recent_anomalies: Vec::new(),
            recent_new_patterns: Vec::new(),
            snapshot_tx,
        }
    }

    /// Run the engine, consuming entries from the channel.
    ///
    /// This method runs forever (until the entry channel is closed) and should
    /// be spawned as a tokio task.
    pub async fn run(mut self, mut entry_rx: mpsc::UnboundedReceiver<LogEntry>) {
        let mut tick_interval = tokio::time::interval(std::time::Duration::from_millis(1000));

        loop {
            tokio::select! {
                maybe_entry = entry_rx.recv() => {
                    match maybe_entry {
                        Some(entry) => self.ingest(entry),
                        None => break, // channel closed
                    }
                }
                _ = tick_interval.tick() => {
                    self.tick();
                }
            }
        }
    }

    /// Ingest a single log entry into all analytics subsystems.
    fn ingest(&mut self, entry: LogEntry) {
        // Cluster the entry.
        let _pattern_idx = self.clusterer.ingest(&entry.line, entry.timestamp);

        // Check for new patterns.
        let top = self.clusterer.top_patterns(1);
        if let Some(pattern) = top.first() {
            if let Some(event) =
                self.pattern_detector
                    .check(&pattern.template, &pattern.example, entry.timestamp)
            {
                self.recent_new_patterns.push(event);
            }
        }

        self.current_tick_entries += 1;

        // Buffer the entry for correlation and trend analysis.
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    /// Called every tick interval to perform periodic analysis.
    fn tick(&mut self) {
        self.tick_count += 1;

        // Volume anomaly check.
        if let Some(anomaly) = self
            .volume_detector
            .record_tick(self.current_tick_entries as f64)
        {
            self.recent_anomalies.push(anomaly);
        }
        self.current_tick_entries = 0;

        // Generate and send a snapshot every 5 ticks.
        if self.tick_count % 5 == 0 {
            let snapshot = AnalyticsSnapshot {
                top_patterns: self
                    .clusterer
                    .top_patterns(20)
                    .into_iter()
                    .cloned()
                    .collect(),
                anomalies: std::mem::take(&mut self.recent_anomalies),
                new_patterns: std::mem::take(&mut self.recent_new_patterns),
                correlation: if !self.entries.is_empty() {
                    Some(self.correlation.analyze(&self.entries))
                } else {
                    None
                },
                trend: self.trend.analyze(&self.entries),
                slowest_endpoints: TopNAnalyzer::slowest_endpoints(&self.entries, 10),
                noisiest_sources: TopNAnalyzer::noisiest(&self.entries, "service", 10),
            };

            // Ignore send errors — the receiver may have been dropped.
            let _ = self.snapshot_tx.send(snapshot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_entry(line: &str) -> LogEntry {
        let mut labels = BTreeMap::new();
        labels.insert("level".to_string(), "info".to_string());
        labels.insert("service".to_string(), "test".to_string());
        LogEntry {
            timestamp: chrono::Utc::now(),
            labels,
            line: line.to_string(),
            raw: None,
        }
    }

    #[test]
    fn engine_creates_and_sends_snapshot() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut engine = AnalyticsEngine::new(tx);

        // Ingest some entries.
        for i in 0..10 {
            engine.ingest(make_entry(&format!(
                "GET /api/v1/resource/{} 200 {}ms",
                i,
                i * 10
            )));
        }

        // Simulate 5 ticks to trigger a snapshot.
        for _ in 0..5 {
            engine.tick();
        }

        // Should have received one snapshot.
        let snapshot = rx.try_recv();
        assert!(snapshot.is_ok(), "expected a snapshot after 5 ticks");
        let snap = snapshot.unwrap();
        assert!(
            !snap.top_patterns.is_empty(),
            "snapshot should contain patterns"
        );
    }
}
