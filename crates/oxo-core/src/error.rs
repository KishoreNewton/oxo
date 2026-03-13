//! Error types for oxo backends.
//!
//! All backend implementations return errors from this module, giving the TUI
//! a single, predictable error surface to handle regardless of which backend
//! is active.

use std::time::Duration;

/// Errors that can occur when communicating with a log backend.
///
/// Each variant carries enough context for the TUI to display a meaningful
/// message to the user (e.g. in the status bar) without needing to know
/// backend-specific details.
#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    /// The backend could not establish a connection.
    #[error("connection failed: {0}")]
    Connection(String),

    /// Authentication was rejected (bad credentials, expired token, etc.).
    #[error("authentication failed: {0}")]
    Auth(String),

    /// The query was malformed or rejected by the backend.
    #[error("query error: {0}")]
    Query(String),

    /// The request timed out after the given duration.
    #[error("timeout after {0:?}")]
    Timeout(Duration),

    /// The backend returned a rate-limit or throttle response.
    #[error("rate limited: retry after {retry_after:?}")]
    RateLimited {
        /// How long the backend suggests waiting before retrying.
        retry_after: Option<Duration>,
    },

    /// Catch-all for unexpected errors.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
