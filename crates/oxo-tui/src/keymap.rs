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

        // Detail view
        KeyCode::Enter => Action::ToggleDetail,

        // Copy current line
        KeyCode::Char('y') => Action::CopyLine,

        // Export logs
        KeyCode::Char('e') => Action::ExportLogs,

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
        _ => Action::Noop,
    }
}
