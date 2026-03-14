//! Layout management and focus cycling.
//!
//! Defines how components are arranged on screen and which component
//! currently has keyboard focus.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Identifiers for focusable components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    /// The query input bar.
    QueryBar,
    /// The main log viewer.
    LogViewer,
    /// The label filter panel (when visible).
    FilterPanel,
    /// The sparkline chart.
    Sparkline,
    /// The histogram chart (replaces sparkline in functionality).
    Histogram,
}

/// The order in which focus cycles through components.
const FOCUS_ORDER: &[FocusTarget] = &[
    FocusTarget::QueryBar,
    FocusTarget::LogViewer,
    FocusTarget::FilterPanel,
    FocusTarget::Sparkline,
    FocusTarget::Histogram,
];

/// Focus state manager.
#[derive(Debug)]
pub struct FocusManager {
    /// Currently focused component.
    current: FocusTarget,
    /// Whether the filter panel is visible (affects focus cycling).
    filter_visible: bool,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusManager {
    /// Create a new focus manager with the log viewer focused.
    pub fn new() -> Self {
        Self {
            current: FocusTarget::LogViewer,
            filter_visible: false,
        }
    }

    /// Get the currently focused component.
    pub fn current(&self) -> FocusTarget {
        self.current
    }

    /// Set focus to a specific target.
    pub fn set(&mut self, target: FocusTarget) {
        self.current = target;
    }

    /// Update whether the filter panel is visible.
    pub fn set_filter_visible(&mut self, visible: bool) {
        self.filter_visible = visible;
        // If the filter panel was focused and is now hidden, move focus.
        if !visible && self.current == FocusTarget::FilterPanel {
            self.current = FocusTarget::LogViewer;
        }
    }

    /// Cycle focus to the next component.
    pub fn next(&mut self) {
        let available = self.available_targets();
        if let Some(pos) = available.iter().position(|t| *t == self.current) {
            let next_pos = (pos + 1) % available.len();
            self.current = available[next_pos];
        }
    }

    /// Cycle focus to the previous component.
    pub fn prev(&mut self) {
        let available = self.available_targets();
        if let Some(pos) = available.iter().position(|t| *t == self.current) {
            let prev_pos = if pos == 0 {
                available.len() - 1
            } else {
                pos - 1
            };
            self.current = available[prev_pos];
        }
    }

    /// Check if a given target is currently focused.
    pub fn is_focused(&self, target: FocusTarget) -> bool {
        self.current == target
    }

    /// Get the list of currently available focus targets.
    fn available_targets(&self) -> Vec<FocusTarget> {
        FOCUS_ORDER
            .iter()
            .filter(|t| {
                if **t == FocusTarget::FilterPanel && !self.filter_visible {
                    return false;
                }
                true
            })
            .copied()
            .collect()
    }
}

/// Computed layout areas for all components.
#[derive(Debug, Clone)]
pub struct AppLayout {
    /// Area for the query bar.
    pub query_bar: Rect,
    /// Area for the tab bar (between query bar and log viewer area).
    pub tab_bar: Rect,
    /// Area for the filter panel (may be zero-width if hidden).
    pub filter_panel: Rect,
    /// Area for the main log viewer.
    pub log_viewer: Rect,
    /// Area for the sparkline chart.
    pub sparkline: Rect,
    /// Area for the histogram chart.
    pub histogram: Rect,
    /// Area for the status bar.
    pub status_bar: Rect,
}

/// Compute the layout areas for the given terminal size.
///
/// The layout adapts based on whether the filter panel is visible:
///
/// ```text
/// With filter:                    Without filter:
/// ┌──────────────────────┐       ┌──────────────────────┐
/// │ QueryBar             │       │ QueryBar             │
/// ├────────┬─────────────┤       ├──────────────────────┤
/// │ Filter │ LogViewer   │       │ LogViewer            │
/// │        │             │       │                      │
/// │        ├─────────────┤       ├──────────────────────┤
/// │        │ Histogram   │       │ Histogram            │
/// ├────────┴─────────────┤       ├──────────────────────┤
/// │ StatusBar            │       │ StatusBar            │
/// └──────────────────────┘       └──────────────────────┘
/// ```
pub fn compute_layout(area: Rect, filter_visible: bool) -> AppLayout {
    // Vertical split: query_bar | tab_bar | middle | histogram | status_bar.
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // query bar
            Constraint::Length(1), // tab bar
            Constraint::Min(5),    // middle (log viewer + optional filter)
            Constraint::Length(5), // histogram (was 4 for sparkline)
            Constraint::Length(1), // status bar
        ])
        .split(area);

    let query_bar = vertical[0];
    let tab_bar = vertical[1];
    let middle = vertical[2];
    let histogram = vertical[3];
    let status_bar = vertical[4];

    // Horizontal split of the middle area (filter panel + log viewer).
    let (filter_panel, log_viewer) = if filter_visible {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(25), // filter panel width
                Constraint::Min(20),    // log viewer
            ])
            .split(middle);
        (horizontal[0], horizontal[1])
    } else {
        (Rect::default(), middle)
    };

    AppLayout {
        query_bar,
        tab_bar,
        filter_panel,
        log_viewer,
        sparkline: histogram,
        histogram,
        status_bar,
    }
}
