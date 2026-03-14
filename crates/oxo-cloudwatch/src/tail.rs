//! Polling-based live tail for CloudWatch Logs.
//!
//! CloudWatch Logs does not offer a persistent streaming connection like
//! Loki's WebSocket tail. Instead, we poll [`FilterLogEvents`] on a fixed
//! interval, advancing the start time forward and deduplicating events by
//! `eventId` to avoid sending the same entry twice.
//!
//! The tail loop runs as a spawned tokio task and sends [`LogEntry`] values
//! through an [`mpsc::UnboundedSender`]. Dropping the corresponding
//! [`TailHandle`] aborts the task automatically.

use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use oxo_core::backend::LogEntry;

use crate::client::CloudWatchClient;
use crate::convert::filtered_event_to_log_entry;

/// Interval between successive poll requests.
const POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Maximum number of event IDs to keep in the deduplication set before
/// pruning the oldest entries.
const MAX_SEEN_IDS: usize = 50_000;

/// Start a polling tail loop for CloudWatch Logs.
///
/// This function runs indefinitely (until the `tx` channel is closed or the
/// task is aborted) and sends new log entries to `tx`.
///
/// # Arguments
///
/// * `client` — A [`CloudWatchClient`] used to issue `FilterLogEvents` calls.
/// * `log_group` — The log group to tail.
/// * `filter_pattern` — Optional CloudWatch filter pattern (may be empty).
/// * `tx` — Channel sender for delivering log entries to the TUI.
pub async fn start_tail(
    client: &CloudWatchClient,
    log_group: &str,
    filter_pattern: Option<&str>,
    tx: mpsc::UnboundedSender<LogEntry>,
) {
    let mut seen_ids: HashSet<String> = HashSet::new();

    // Start tailing from 30 seconds ago to catch any recent events.
    let mut start_time = Utc::now() - chrono::Duration::seconds(30);

    loop {
        if tx.is_closed() {
            debug!("tail channel closed, stopping CloudWatch tail");
            return;
        }

        let end_time = Utc::now();
        let start_ms = start_time.timestamp_millis();
        let end_ms = end_time.timestamp_millis();

        match client
            .filter_log_events(log_group, filter_pattern, start_ms, end_ms, 1000, None)
            .await
        {
            Ok(response) => {
                let mut latest_ts: Option<DateTime<Utc>> = None;

                for event in response.events {
                    // Deduplicate by event ID.
                    let event_id = match &event.event_id {
                        Some(id) => id.clone(),
                        None => continue,
                    };

                    if seen_ids.contains(&event_id) {
                        continue;
                    }

                    if let Some(entry) =
                        filtered_event_to_log_entry(&event, log_group)
                    {
                        // Track the latest timestamp we've seen.
                        if latest_ts.is_none() || entry.timestamp > latest_ts.unwrap() {
                            latest_ts = Some(entry.timestamp);
                        }

                        if tx.send(entry).is_err() {
                            debug!("tail receiver dropped, stopping");
                            return;
                        }
                    }

                    seen_ids.insert(event_id);
                }

                // Advance start time to avoid re-fetching old events.
                // We subtract 1 second of overlap to handle clock skew and
                // events that arrive slightly out of order.
                if let Some(ts) = latest_ts {
                    let new_start = ts - chrono::Duration::seconds(1);
                    if new_start > start_time {
                        start_time = new_start;
                    }
                }

                // Prune dedup set if it's grown too large.
                if seen_ids.len() > MAX_SEEN_IDS {
                    debug!("pruning dedup set ({} entries)", seen_ids.len());
                    seen_ids.clear();
                }
            }
            Err(e) => {
                warn!("CloudWatch tail poll failed: {e}");
                // On rate-limiting, back off a bit more.
                if matches!(e, oxo_core::error::BackendError::RateLimited { .. }) {
                    error!("rate limited during tail, backing off");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
