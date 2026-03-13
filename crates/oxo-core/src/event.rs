//! Domain events flowing from backends to the TUI.
//!
//! The TUI listens for these events on a channel and updates its state
//! accordingly. This decouples backend I/O from rendering.

use crate::backend::LogEntry;
use crate::error::BackendError;

/// Events emitted by a backend during operation.
///
/// The TUI's main loop receives these through a channel and maps them to
/// UI updates (e.g. appending log lines, showing a reconnection indicator).
#[derive(Debug)]
pub enum BackendEvent {
    /// A batch of new log entries arrived (from a query or live tail).
    LogsBatch(Vec<LogEntry>),

    /// An error occurred in the backend.
    Error(BackendError),

    /// The backend successfully connected to the remote server.
    Connected,

    /// The backend lost its connection.
    Disconnected,

    /// The backend is attempting to reconnect.
    Reconnecting {
        /// Which reconnection attempt this is (1-based).
        attempt: u32,
    },
}
