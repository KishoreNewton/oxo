//! WebSocket live-tailing for Loki.
//!
//! Opens a WebSocket connection to Loki's `/loki/api/v1/tail` endpoint
//! and streams log entries into an [`mpsc::UnboundedSender`] channel.
//!
//! Handles automatic reconnection with exponential backoff when the
//! connection drops unexpectedly.

use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tracing::{error, info, warn};
use url::Url;

use oxo_core::backend::LogEntry;
use oxo_core::config::AuthConfig;

use crate::response::TailFrame;

/// Maximum number of reconnection attempts before giving up.
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Base delay for exponential backoff (doubles each attempt).
const BASE_RECONNECT_DELAY: Duration = Duration::from_secs(1);

/// Maximum delay between reconnection attempts.
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

/// Start a live tail stream over WebSocket.
///
/// This function connects to Loki's tail endpoint and spawns an internal
/// loop that deserializes incoming frames and forwards [`LogEntry`] values
/// through the provided sender.
///
/// # Arguments
///
/// * `base_url` — The Loki base URL (will be converted to `ws://` or `wss://`).
/// * `query` — The LogQL query to tail.
/// * `auth` — Optional authentication configuration.
/// * `org_id` — Optional tenant ID for multi-tenant Loki.
/// * `tx` — Channel sender for delivering log entries to the TUI.
///
/// # Returns
///
/// Returns `Ok(())` when the connection is established and the streaming
/// loop is running. The loop will continue until `tx` is closed (receiver
/// dropped) or reconnection attempts are exhausted.
pub async fn start_tail(
    base_url: &Url,
    query: &str,
    auth: &Option<AuthConfig>,
    org_id: Option<&str>,
    tx: mpsc::UnboundedSender<LogEntry>,
) -> Result<(), oxo_core::error::BackendError> {
    let ws_url = build_ws_url(base_url, query)?;

    let mut attempt: u32 = 0;

    loop {
        // Check if the receiver has been dropped — no point reconnecting.
        if tx.is_closed() {
            info!("tail channel closed, stopping");
            return Ok(());
        }

        match connect_and_stream(&ws_url, auth, org_id, &tx).await {
            Ok(()) => {
                // Clean disconnect (e.g. server closed gracefully).
                info!("tail WebSocket closed normally");
                return Ok(());
            }
            Err(e) => {
                attempt += 1;
                if attempt > MAX_RECONNECT_ATTEMPTS {
                    error!("tail reconnection attempts exhausted ({MAX_RECONNECT_ATTEMPTS})");
                    return Err(oxo_core::error::BackendError::Connection(format!(
                        "WebSocket reconnection failed after {MAX_RECONNECT_ATTEMPTS} attempts: {e}"
                    )));
                }

                let delay = backoff_delay(attempt);
                warn!(
                    "tail WebSocket error (attempt {attempt}/{MAX_RECONNECT_ATTEMPTS}), \
                     reconnecting in {delay:?}: {e}"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Connect to the WebSocket and stream entries until disconnect.
async fn connect_and_stream(
    ws_url: &Url,
    auth: &Option<AuthConfig>,
    org_id: Option<&str>,
    tx: &mpsc::UnboundedSender<LogEntry>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut request = ws_url.as_str().into_client_request()?;

    // Apply authentication headers.
    let headers = request.headers_mut();
    match auth {
        Some(AuthConfig::Basic { username, password }) => {
            use base64::Engine;
            let credentials =
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
            let value = format!("Basic {credentials}")
                .parse()
                .map_err(|e| format!("invalid Authorization header value: {e}"))?;
            headers.insert("Authorization", value);
        }
        Some(AuthConfig::Bearer { token }) => {
            let value = format!("Bearer {token}")
                .parse()
                .map_err(|e| format!("invalid Authorization header value: {e}"))?;
            headers.insert("Authorization", value);
        }
        None => {}
    }

    // Multi-tenant header.
    if let Some(org_id) = org_id {
        let value = org_id
            .parse()
            .map_err(|e| format!("invalid X-Scope-OrgID header value: {e}"))?;
        headers.insert("X-Scope-OrgID", value);
    }

    let (ws_stream, _response) = tokio_tungstenite::connect_async(request).await?;
    info!("tail WebSocket connected");

    let (_write, mut read) = ws_stream.split();

    while let Some(msg_result) = read.next().await {
        let msg = msg_result?;

        match msg {
            Message::Text(text) => {
                match serde_json::from_str::<TailFrame>(&text) {
                    Ok(frame) => {
                        let entries = frame.into_log_entries();
                        for entry in entries {
                            if tx.send(entry).is_err() {
                                // Receiver dropped — stop tailing.
                                return Ok(());
                            }
                        }
                    }
                    Err(e) => {
                        warn!("failed to parse tail frame: {e}");
                    }
                }
            }
            Message::Close(_) => {
                info!("tail WebSocket received close frame");
                return Ok(());
            }
            // Ping/Pong are handled automatically by tungstenite.
            _ => {}
        }
    }

    Ok(())
}

/// Build the WebSocket URL for tailing.
///
/// Converts `http://` → `ws://` and `https://` → `wss://`, then appends
/// the tail path and query parameter.
fn build_ws_url(base_url: &Url, query: &str) -> Result<Url, oxo_core::error::BackendError> {
    let scheme = match base_url.scheme() {
        "http" => "ws",
        "https" => "wss",
        other => {
            return Err(oxo_core::error::BackendError::Connection(format!(
                "unsupported URL scheme: {other}"
            )));
        }
    };

    let mut ws_url = base_url.clone();
    ws_url
        .set_scheme(scheme)
        .map_err(|_| oxo_core::error::BackendError::Connection("failed to set WS scheme".into()))?;

    ws_url.set_path("/loki/api/v1/tail");
    ws_url.query_pairs_mut().append_pair("query", query);

    Ok(ws_url)
}

/// Calculate exponential backoff delay with a cap.
fn backoff_delay(attempt: u32) -> Duration {
    let delay = BASE_RECONNECT_DELAY * 2u32.saturating_pow(attempt.saturating_sub(1));
    delay.min(MAX_RECONNECT_DELAY)
}
