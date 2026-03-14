//! # oxo-analytics
//!
//! Advanced log analytics for the **oxo** observability TUI.
//!
//! This crate provides pure-Rust implementations of:
//!
//! - **Pattern clustering** ([`clustering`]) — Drain-inspired algorithm that
//!   automatically discovers log templates by replacing variable parts with
//!   `{*}` wildcards.
//! - **Anomaly detection** ([`anomaly`]) — Z-score volume spike detection and
//!   new-pattern detection after a learning phase.
//! - **Error correlation** ([`correlation`]) — identifies which label values
//!   correlate with error rate increases.
//! - **Trend analysis** ([`trend`]) — linear regression on error rate over
//!   time to detect increasing/decreasing trends.
//! - **Top-N analysis** ([`topn`]) — noisiest sources, slowest endpoints,
//!   and top error producers.
//! - **Analytics engine** ([`engine`]) — orchestrator that ties everything
//!   together, consuming log entries from a channel and emitting periodic
//!   snapshots.
//!
//! All algorithms are zero-dependency on ML libraries — pure Rust with
//! basic statistics.

pub mod anomaly;
pub mod clustering;
pub mod correlation;
pub mod engine;
pub mod metrics;
pub mod topn;
pub mod trend;
