//! Internal action/command enum — the TUI's message bus.
//!
//! Components communicate with each other and with the [`App`](crate::app::App)
//! by emitting [`Action`] values. The main loop dispatches these actions,
//! updating state and triggering re-renders as needed.

use oxo_core::BackendEvent;

/// An action that can be dispatched through the TUI.
///
/// Actions are the sole mechanism for state transitions. Components produce
/// them in response to key presses; the main loop consumes them.
#[derive(Debug)]
pub enum Action {
    // ── Lifecycle ────────────────────────────────────────────────────
    /// Quit the application.
    Quit,

    /// The terminal was resized to the given dimensions.
    Resize { width: u16, height: u16 },

    /// Periodic tick (used for sparkline rate calculations).
    Tick,

    // ── Navigation / focus ──────────────────────────────────────────
    /// Cycle focus to the next component.
    FocusNext,

    /// Cycle focus to the previous component.
    FocusPrev,

    /// Switch to the query input bar.
    EnterQueryMode,

    /// Exit query mode and return to normal log browsing.
    ExitQueryMode,

    /// Toggle the help overlay.
    ToggleHelp,

    /// Toggle the filter side panel.
    ToggleFilterPanel,

    // ── Search ──────────────────────────────────────────────────────
    /// Enter search mode (open search input).
    EnterSearchMode,

    /// Exit search mode.
    ExitSearchMode,

    /// Set the active search term and highlight matches.
    SearchSubmit(String),

    /// Jump to the next search match.
    SearchNext,

    /// Jump to the previous search match.
    SearchPrev,

    /// Clear the search term and highlights.
    SearchClear,

    // ── Query / filter ──────────────────────────────────────────────
    /// Submit a query string to the backend.
    SubmitQuery(String),

    /// Toggle a label filter on/off and rebuild the query.
    SetFilter { label: String, value: String },

    /// Remove all active label filters.
    ClearFilters,

    // ── Log viewer ──────────────────────────────────────────────────
    /// Scroll the log viewer up by N lines.
    ScrollUp(usize),

    /// Scroll the log viewer down by N lines.
    ScrollDown(usize),

    /// Jump to the top of the log buffer.
    ScrollToTop,

    /// Jump to the bottom (resume auto-scroll / tail mode).
    ScrollToBottom,

    /// Page up in the log viewer.
    PageUp,

    /// Page down in the log viewer.
    PageDown,

    /// Toggle line wrapping in the log viewer.
    ToggleLineWrap,

    /// Toggle timestamp display in the log viewer.
    ToggleTimestamps,

    /// Copy the currently selected log line to clipboard.
    CopyLine,

    /// Select a log line (by index in visible area).
    SelectLine(usize),

    /// Toggle the detail/inspect panel for the selected log line.
    ToggleDetail,

    // ── Mouse ───────────────────────────────────────────────────────
    /// Mouse scroll up.
    MouseScrollUp(u16, u16),

    /// Mouse scroll down.
    MouseScrollDown(u16, u16),

    /// Mouse click at a position.
    MouseClick(u16, u16),

    // ── Notifications ───────────────────────────────────────────────
    /// Show a notification message in the status bar (auto-clears).
    Notify(String),

    /// Show an error notification.
    NotifyError(String),

    // ── Export ───────────────────────────────────────────────────────
    /// Export visible logs to a file.
    ExportLogs,

    // ── Backend events ──────────────────────────────────────────────
    /// An event received from the active backend.
    Backend(BackendEvent),

    // ── No-op ───────────────────────────────────────────────────────
    /// Do nothing (used as a default / placeholder).
    Noop,
}
