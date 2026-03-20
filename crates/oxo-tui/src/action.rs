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

    /// Toggle the time range picker overlay.
    ToggleTimePicker,

    // ── Time range ──────────────────────────────────────────────────
    /// Set the active time range to the given duration in minutes.
    SetTimeRange(u64),

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
    /// Export visible logs to a file (JSON format).
    ExportLogs,

    /// Export visible logs as CSV.
    ExportCsv,

    /// Export visible logs as NDJSON (newline-delimited JSON).
    ExportNdjson,

    // ── Backend events ──────────────────────────────────────────────
    /// An event received from the active backend.
    Backend(BackendEvent),

    // ── Statistics ──────────────────────────────────────────────────
    /// Toggle the log statistics overlay.
    ToggleStats,

    // ── Label values ────────────────────────────────────────────────
    /// Request values for a label from the backend (resolved asynchronously).
    LoadLabelValues(String),

    // ── Saved queries ───────────────────────────────────────────────
    /// Save the current query with a name (empty = auto-generate from timestamp).
    SaveQuery(String),

    // ── Sources ──────────────────────────────────────────────────────
    /// Toggle the source picker overlay.
    ToggleSourcePicker,

    /// Switch to the source with the given name.
    SwitchSource(String),

    // ── Tabs ────────────────────────────────────────────────────────
    /// Open a new tab with the default query.
    NewTab,

    /// Close the currently active tab.
    CloseTab,

    /// Switch to the tab at the given 0-based index.
    SwitchTab(usize),

    // ── Multi-line / context ──────────────────────────────────────────
    /// Toggle expand/collapse of a multi-line log group (stack trace, etc.).
    ToggleExpand,

    /// Cycle the search context lines setting (0 → 3 → 5 → 10 → 0).
    ToggleContext,

    // ── Alerts ───────────────────────────────────────────────────────
    /// An alert rule fired.
    AlertFired { rule_name: String, message: String },

    /// Toggle the alert history overlay.
    ToggleAlertPanel,

    /// Mute/unmute all alerts.
    ToggleAlertMute,

    // ── Analytics ────────────────────────────────────────────────────
    /// Toggle the analytics dashboard overlay.
    ToggleAnalytics,

    // ── Column mode ──────────────────────────────────────────────────
    /// Toggle column/table view mode for structured log entries.
    ToggleColumnMode,

    /// Sort the column view by the column at the given index.
    SortColumn(usize),

    // ── Dedup ──────────────────────────────────────────────────────
    /// Cycle dedup mode: Off → Exact (global identical) → Fuzzy (normalized similarity) → Off.
    ToggleDedup,

    // ── Bookmarks ──────────────────────────────────────────────────
    /// Toggle a bookmark on the currently selected log entry.
    ToggleBookmark,

    /// Jump to the next bookmarked entry.
    NextBookmark,

    /// Jump to the previous bookmarked entry.
    PrevBookmark,

    /// Clear all bookmarks.
    ClearBookmarks,

    // ── Health ───────────────────────────────────────────────────────
    /// Toggle the health dashboard overlay.
    ToggleHealthDashboard,

    // ── Autocomplete ──────────────────────────────────────────────────
    /// Request available label names from the backend for autocomplete.
    AutocompleteLabels,

    /// Request values for the given label name from the backend for autocomplete.
    AutocompleteLabelValues(String),

    // ── Trace / Regex ───────────────────────────────────────────────
    /// Toggle the trace waterfall overlay.
    ToggleTraceWaterfall,

    /// Toggle the regex playground overlay.
    ToggleRegexPlayground,

    // ── Diff mode ────────────────────────────────────────────────────
    /// Toggle the live diff mode overlay.
    ToggleDiffMode,

    /// Set or update the left-hand diff query.
    DiffQueryLeft(String),

    /// Set or update the right-hand diff query.
    DiffQueryRight(String),

    // ── Saved views ──────────────────────────────────────────────────
    /// Toggle the saved views overlay.
    ToggleSavedViews,

    // ── Live dashboard ─────────────────────────────────────────────
    /// Toggle the live metrics dashboard overlay.
    ToggleLiveDashboard,

    // ── Incident timeline ───────────────────────────────────────────
    /// Toggle the incident timeline overlay.
    ToggleIncidentTimeline,

    /// Mark current time as an incident boundary.
    MarkIncident(String),

    // ── Natural language query ───────────────────────────────────────
    /// Toggle the natural language query overlay.
    ToggleNlQuery,

    // ── No-op ───────────────────────────────────────────────────────
    /// Do nothing (used as a default / placeholder).
    Noop,
}
