//! The main log viewer component.
//!
//! Displays a scrollable list of log entries with timestamp coloring,
//! log-level highlighting, and auto-scroll (tail mode). When the user
//! scrolls up, tail mode pauses and a "N new lines" indicator appears.

use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use oxo_core::LogEntry;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Log viewer component state.
pub struct LogViewer {
    /// Reference to the shared log buffer (read-only view).
    ///
    /// The `App` owns the actual buffer; the viewer receives a snapshot
    /// via [`update_entries`](LogViewer::update_entries).
    entries: Vec<LogEntry>,

    /// Current scroll offset (0 = bottom / most recent).
    scroll_offset: usize,

    /// Whether we are in tail mode (auto-scroll to bottom).
    tail_mode: bool,

    /// Number of new entries received while scrolled away from bottom.
    new_entries_count: usize,

    /// Whether to show timestamps.
    show_timestamps: bool,

    /// Whether to wrap long lines.
    line_wrap: bool,

    /// Height of the viewport (set during render).
    viewport_height: usize,

    /// The color theme.
    theme: Theme,
}

impl LogViewer {
    /// Create a new log viewer with default settings.
    pub fn new(theme: Theme) -> Self {
        Self {
            entries: Vec::new(),
            scroll_offset: 0,
            tail_mode: true,
            new_entries_count: 0,
            show_timestamps: true,
            line_wrap: false,
            viewport_height: 0,
            theme,
        }
    }

    /// Update the entries displayed by this viewer.
    ///
    /// Called by the `App` whenever the log buffer changes.
    pub fn update_entries(&mut self, buffer: &VecDeque<LogEntry>) {
        let previous_len = self.entries.len();
        self.entries = buffer.iter().cloned().collect();

        if self.tail_mode {
            // Stay at the bottom.
            self.scroll_offset = 0;
        } else {
            // Track how many new entries arrived while scrolled away.
            let new_count = self.entries.len().saturating_sub(previous_len);
            self.new_entries_count += new_count;
            // Adjust scroll offset to keep the view stable.
            self.scroll_offset += new_count;
        }
    }

    /// Scroll up by N lines.
    fn scroll_up(&mut self, n: usize) {
        let max_offset = self.entries.len().saturating_sub(self.viewport_height);
        self.scroll_offset = (self.scroll_offset + n).min(max_offset);
        self.tail_mode = false;
    }

    /// Scroll down by N lines.
    fn scroll_down(&mut self, n: usize) {
        if self.scroll_offset <= n {
            self.scroll_offset = 0;
            self.tail_mode = true;
            self.new_entries_count = 0;
        } else {
            self.scroll_offset -= n;
        }
    }

    /// Jump to the top of the buffer.
    fn scroll_to_top(&mut self) {
        let max_offset = self.entries.len().saturating_sub(self.viewport_height);
        self.scroll_offset = max_offset;
        self.tail_mode = false;
    }

    /// Jump to the bottom (resume tail mode).
    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.tail_mode = true;
        self.new_entries_count = 0;
    }

    /// Format a single log entry as a styled [`Line`].
    fn format_entry(&self, entry: &LogEntry) -> Line<'_> {
        let mut spans = Vec::new();

        // Timestamp.
        if self.show_timestamps {
            let ts = entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
            spans.push(Span::styled(ts, self.theme.dimmed()));
            spans.push(Span::raw(" "));
        }

        // Log level (if present in labels).
        if let Some(level) = entry.labels.get("level").or(entry.labels.get("severity")) {
            let style = self.theme.log_level_style(level);
            spans.push(Span::styled(format!("[{level:>5}]"), style));
            spans.push(Span::raw(" "));
        }

        // Log line.
        spans.push(Span::styled(entry.line.clone(), Style::default()));

        Line::from(spans)
    }
}

impl Component for LogViewer {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_down(1);
                Some(Action::Noop)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_up(1);
                Some(Action::Noop)
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.scroll_to_top();
                Some(Action::Noop)
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.scroll_to_bottom();
                Some(Action::Noop)
            }
            KeyCode::PageDown => {
                self.scroll_down(self.viewport_height.saturating_sub(2));
                Some(Action::Noop)
            }
            KeyCode::PageUp => {
                self.scroll_up(self.viewport_height.saturating_sub(2));
                Some(Action::Noop)
            }
            _ => None,
        }
    }

    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        match action {
            Action::ToggleLineWrap => {
                self.line_wrap = !self.line_wrap;
                None
            }
            Action::ToggleTimestamps => {
                self.show_timestamps = !self.show_timestamps;
                None
            }
            _ => None,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        // Record viewport height for scroll calculations.
        // We subtract 2 for the block borders.
        let inner_height = area.height.saturating_sub(2) as usize;

        // Build the title with tail/scroll indicator.
        let title = if self.tail_mode {
            " Logs (TAIL) ".to_string()
        } else if self.new_entries_count > 0 {
            format!(" Logs (+{} new) ", self.new_entries_count)
        } else {
            " Logs ".to_string()
        };

        let border_style = if focused {
            self.theme.border_focused()
        } else {
            self.theme.border_unfocused()
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        // Get the visible slice of entries.
        let total = self.entries.len();
        let start = if total > inner_height + self.scroll_offset {
            total - inner_height - self.scroll_offset
        } else {
            0
        };
        let end = total.saturating_sub(self.scroll_offset);

        let lines: Vec<Line> = self.entries[start..end]
            .iter()
            .map(|entry| self.format_entry(entry))
            .collect();

        let mut paragraph = Paragraph::new(lines).block(block);
        if self.line_wrap {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }

        frame.render_widget(paragraph, area);

        // SAFETY: We need to update viewport_height for scroll calculations,
        // but render takes &self. The App updates this via a separate method
        // or we accept the one-frame delay.
        // In practice this is fine since the value stabilizes after the first render.
        let _ = inner_height; // Used above; stored by App separately.
    }
}

// Allow the App to set viewport height after render.
impl LogViewer {
    /// Set the viewport height (called by App after layout is computed).
    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height;
    }
}
