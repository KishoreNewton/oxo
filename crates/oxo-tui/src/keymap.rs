//! Key binding definitions.
//!
//! Maps key events to [`Action`]s based on the current application mode.
//! Follows vim-style navigation conventions by default.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;

/// The current input mode, which determines how key presses are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal mode — browsing logs, navigating panels.
    Normal,
    /// Query mode — typing in the query bar.
    Query,
    /// Search mode — typing in the search bar.
    Search,
    /// Filter mode — navigating the filter panel.
    Filter,
    /// Detail mode — viewing a log entry's details.
    Detail,
}

/// Map a key event to an action based on the current input mode.
pub fn handle_key(mode: InputMode, key: KeyEvent) -> Action {
    // Global bindings (work in any mode).
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('q') => return Action::Quit,
            _ => {}
        }
    }

    match mode {
        InputMode::Normal => handle_normal_mode(key),
        InputMode::Query => handle_query_mode(key),
        InputMode::Search => handle_search_mode(key),
        InputMode::Filter => handle_filter_mode(key),
        InputMode::Detail => handle_detail_mode(key),
    }
}

/// Key bindings for normal (log browsing) mode.
fn handle_normal_mode(key: KeyEvent) -> Action {
    // Ctrl+t → new tab; Ctrl+w → close tab.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('t') => return Action::NewTab,
            KeyCode::Char('w') => return Action::CloseTab,
            _ => {}
        }
    }

    match key.code {
        // Quit
        KeyCode::Char('q') => Action::Quit,

        // Vertical scrolling (vim-style)
        KeyCode::Char('j') | KeyCode::Down => Action::ScrollDown(1),
        KeyCode::Char('k') | KeyCode::Up => Action::ScrollUp(1),

        // Page scrolling
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::PageDown,
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::PageUp => Action::PageUp,

        // Jump to top/bottom
        KeyCode::Char('g') => Action::ScrollToTop,
        KeyCode::Char('G') => Action::ScrollToBottom,
        KeyCode::Home => Action::ScrollToTop,
        KeyCode::End => Action::ScrollToBottom,

        // Search
        KeyCode::Char('/') => Action::EnterSearchMode,
        KeyCode::Char('n') => Action::SearchNext,
        KeyCode::Char('N') => Action::SearchPrev,

        // Enter query mode
        KeyCode::Char(':') => Action::EnterQueryMode,

        // Focus cycling
        KeyCode::Tab => Action::FocusNext,
        KeyCode::BackTab => Action::FocusPrev,

        // Toggles
        KeyCode::Char('?') => Action::ToggleHelp,
        KeyCode::Char('f') => Action::ToggleFilterPanel,
        KeyCode::Char('w') => Action::ToggleLineWrap,
        KeyCode::Char('t') => Action::ToggleTimestamps,
        KeyCode::Char('T') => Action::ToggleTimePicker,

        // Detail view
        KeyCode::Enter => Action::ToggleDetail,

        // Copy current line
        KeyCode::Char('y') => Action::CopyLine,

        // Export logs
        KeyCode::Char('e') => Action::ExportLogs,

        // Switch to tab 1-9 (digit keys, no modifiers).
        KeyCode::Char(c @ '1'..='9') if key.modifiers.is_empty() => {
            let n = (c as u8 - b'0') as usize;
            Action::SwitchTab(n - 1) // convert to 0-based index
        }

        // Source picker
        KeyCode::Char('b') => Action::ToggleSourcePicker,

        // Expand/collapse multi-line log groups
        KeyCode::Char('x') => Action::ToggleExpand,

        // Cycle search context lines
        KeyCode::Char('C') => Action::ToggleContext,

        // Statistics overlay
        KeyCode::Char('s') => Action::ToggleStats,

        // Save current query
        KeyCode::Char('S') => Action::SaveQuery(String::new()),

        // Alert history overlay
        KeyCode::Char('a') => Action::ToggleAlertPanel,

        // Mute/unmute alerts
        KeyCode::Char('A') => Action::ToggleAlertMute,

        // Analytics dashboard overlay
        KeyCode::Char('i') => Action::ToggleAnalytics,

        // Column/table mode
        KeyCode::Char('c') => Action::ToggleColumnMode,

        // Smart deduplication
        KeyCode::Char('D') => Action::ToggleDedup,

        // Bookmarks
        KeyCode::Char('m') => Action::ToggleBookmark,
        KeyCode::Char('\'') => Action::NextBookmark,

        // Trace waterfall
        KeyCode::Char('W') => Action::ToggleTraceWaterfall,

        // Regex playground
        KeyCode::Char('R') => Action::ToggleRegexPlayground,

        // Diff mode
        KeyCode::Char('d') if key.modifiers.is_empty() => Action::ToggleDiffMode,

        // Health dashboard
        KeyCode::Char('H') => Action::ToggleHealthDashboard,

        // Saved views
        KeyCode::Char('V') => Action::ToggleSavedViews,

        // Live metrics dashboard
        KeyCode::Char('L') => Action::ToggleLiveDashboard,

        // Incident timeline
        KeyCode::Char('I') => Action::ToggleIncidentTimeline,

        // Natural language query (Ctrl+l)
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Action::ToggleNlQuery
        }

        _ => Action::Noop,
    }
}

/// Key bindings for query input mode.
fn handle_query_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::ExitQueryMode,
        _ => Action::Noop,
    }
}

/// Key bindings for search input mode.
fn handle_search_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::ExitSearchMode,
        _ => Action::Noop,
    }
}

/// Key bindings for filter panel mode.
fn handle_filter_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::ToggleFilterPanel,
        KeyCode::Char('j') | KeyCode::Down => Action::ScrollDown(1),
        KeyCode::Char('k') | KeyCode::Up => Action::ScrollUp(1),
        _ => Action::Noop,
    }
}

/// Key bindings for detail panel mode.
fn handle_detail_mode(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => Action::ToggleDetail,
        KeyCode::Char('j') | KeyCode::Down => Action::ScrollDown(1),
        KeyCode::Char('k') | KeyCode::Up => Action::ScrollUp(1),
        KeyCode::Char('x') => Action::ToggleExpand,
        _ => Action::Noop,
    }
}
