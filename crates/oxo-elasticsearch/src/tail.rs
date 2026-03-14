//! Polling-based live tail for Elasticsearch.
//!
//! Elasticsearch does not support native push-based streaming like Loki's
//! WebSocket tail. Instead, this module polls the search API at regular
//! intervals using `search_after` cursor-based pagination to discover
//! new documents.
//!
//! The poll loop:
//! 1. Executes a search sorted by `@timestamp` ascending with `search_after`
//!    set to the sort values of the last seen hit.
//! 2. Converts new hits into [`LogEntry`] values and sends them through
//!    the channel.
//! 3. Sleeps for the poll interval (2 seconds) before repeating.
//!
//! On transient errors, the loop retries with exponential backoff
//! (1s → 2s → 4s → ... → 30s max) before resuming normal polling.

use std::time::Duration;

use chrono::Utc;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{info, warn};

use oxo_core::backend::LogEntry;

use crate::client::ElasticsearchClient;

/// How often to poll Elasticsearch for new documents.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Base delay for exponential backoff on errors.
const BASE_BACKOFF: Duration = Duration::from_secs(1);

/// Maximum backoff delay.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Start a polling-based tail loop.
///
/// Continuously polls Elasticsearch for new documents matching `query`
/// in the given `index`. New entries are sent through `tx`.
///
/// The loop terminates when:
/// - The channel receiver is dropped (`tx.is_closed()`).
/// - The task is aborted via the [`TailHandle`](oxo_core::backend::TailHandle).
///
/// # Arguments
///
/// * `client` — The Elasticsearch client to use for queries.
/// * `index` — The index pattern to search.
/// * `query` — The user query string (empty → match_all).
/// * `tx` — Channel sender for delivering log entries.
pub async fn start_tail(
    client: &ElasticsearchClient,
    index: &str,
    query: &str,
    tx: mpsc::UnboundedSender<LogEntry>,
) {
    info!("starting Elasticsearch tail on index={index}");

    // Start tailing from "now" so we only get new documents.
    let tail_start = Utc::now();
    let mut cursor: Option<Vec<Value>> = None;
    let mut consecutive_errors: u32 = 0;

    loop {
        // Check if the receiver has been dropped.
        if tx.is_closed() {
            info!("tail channel closed, stopping poll loop");
            return;
        }

        let now = Utc::now();
        let search_after = cursor.as_deref();

        match client
            .scroll_search(index, query, tail_start, now, search_after)
            .await
        {
            Ok(hits) => {
                // Reset backoff on success.
                consecutive_errors = 0;

                if !hits.is_empty() {
                    // Update cursor to the sort values of the last hit.
                    if let Some(last) = hits.last() {
                        if let Some(sort_vals) = &last.sort {
                            cursor = Some(sort_vals.clone());
                        }
                    }

                    // Convert and send each hit.
                    for hit in hits {
                        let entry = hit.into_log_entry();
                        if tx.send(entry).is_err() {
                            // Receiver dropped.
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                consecutive_errors += 1;
                let delay = backoff_delay(consecutive_errors);
                warn!(
                    "tail poll error (consecutive={consecutive_errors}), \
                     retrying in {delay:?}: {e}"
                );
                tokio::time::sleep(delay).await;
                continue; // Skip the normal poll interval sleep.
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Calculate exponential backoff delay with a cap.
fn backoff_delay(attempt: u32) -> Duration {
    let delay = BASE_BACKOFF * 2u32.saturating_pow(attempt.saturating_sub(1));
    delay.min(MAX_BACKOFF)
}
