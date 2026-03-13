//! The main log viewer component.
//!
//! Displays a scrollable list of log entries with:
//! - Timestamp coloring and log-level highlighting
//! - Auto-scroll (tail mode) with "N new lines" indicator
//! - Live search with match highlighting and n/N navigation
//! - Line selection for detail/inspect view
//! - Mouse scroll support

use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use oxo_core::LogEntry;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Log viewer component state.
pub struct LogViewer {
    /// Snapshot of the log buffer entries.
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

    /// Currently selected line index (relative to visible entries).
    selected_line: Option<usize>,

    /// Active search term.
    search_term: Option<String>,

    /// Indices of entries matching the search term.
    search_matches: Vec<usize>,

    /// Current match index within search_matches.
    search_match_cursor: usize,

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
            selected_line: None,
            search_term: None,
            search_matches: Vec::new(),
            search_match_cursor: 0,
            theme,
        }
    }

    /// Update the entries displayed by this viewer.
    pub fn update_entries(&mut self, buffer: &VecDeque<LogEntry>) {
        let previous_len = self.entries.len();
        self.entries = buffer.iter().cloned().collect();

        if self.tail_mode {
            self.scroll_offset = 0;
        } else {
            let new_count = self.entries.len().saturating_sub(previous_len);
            self.new_entries_count += new_count;
            self.scroll_offset += new_count;
        }

        // Rebuild search matches if there's an active search.
        if self.search_term.is_some() {
            self.rebuild_search_matches();
        }
    }

    /// Set the viewport height (called by App after layout is computed).
    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height;
    }

    /// Get the currently selected log entry (for detail view / copy).
    pub fn selected_entry(&self) -> Option<&LogEntry> {
        let selected = self.selected_line?;
        let (start, _end) = self.visible_range();
        let idx = start + selected;
        self.entries.get(idx)
    }

    /// Get the number of search matches.
    pub fn search_match_count(&self) -> usize {
        self.search_matches.len()
    }

    /// Get the current search match cursor (1-based for display).
    pub fn search_match_position(&self) -> usize {
        if self.search_matches.is_empty() {
            0
        } else {
            self.search_match_cursor + 1
        }
    }

    /// Get the active search term.
    pub fn search_term(&self) -> Option<&str> {
        self.search_term.as_deref()
    }

    /// Set the search term and rebuild matches.
    pub fn set_search(&mut self, term: String) {
        if term.is_empty() {
            self.clear_search();
            return;
        }
        self.search_term = Some(term);
        self.rebuild_search_matches();
        self.search_match_cursor = 0;
        // Jump to first match if any.
        if !self.search_matches.is_empty() {
            self.scroll_to_match(0);
        }
    }

    /// Clear search highlighting.
    pub fn clear_search(&mut self) {
        self.search_term = None;
        self.search_matches.clear();
        self.search_match_cursor = 0;
    }

    /// Jump to the next search match.
    pub fn search_next(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_cursor = (self.search_match_cursor + 1) % self.search_matches.len();
        self.scroll_to_match(self.search_match_cursor);
    }

    /// Jump to the previous search match.
    pub fn search_prev(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        if self.search_match_cursor == 0 {
            self.search_match_cursor = self.search_matches.len() - 1;
        } else {
            self.search_match_cursor -= 1;
        }
        self.scroll_to_match(self.search_match_cursor);
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

    /// Compute the visible (start, end) range of entries.
    fn visible_range(&self) -> (usize, usize) {
        let total = self.entries.len();
        let start = if total > self.viewport_height + self.scroll_offset {
            total - self.viewport_height - self.scroll_offset
        } else {
            0
        };
        let end = total.saturating_sub(self.scroll_offset);
        (start, end)
    }

    /// Rebuild the list of entry indices matching the search term.
    fn rebuild_search_matches(&mut self) {
        self.search_matches.clear();
        if let Some(ref term) = self.search_term {
            let term_lower = term.to_lowercase();
            for (i, entry) in self.entries.iter().enumerate() {
                if entry.line.to_lowercase().contains(&term_lower) {
                    self.search_matches.push(i);
                }
            }
        }
    }

    /// Scroll so that the match at the given index is visible.
    fn scroll_to_match(&mut self, match_idx: usize) {
        if let Some(&entry_idx) = self.search_matches.get(match_idx) {
            let total = self.entries.len();
            // We want entry_idx to be within the visible window.
            // visible range: [total - viewport - offset, total - offset)
            // So offset = total - entry_idx - viewport/2 (center it).
            let half = self.viewport_height / 2;
            if total > self.viewport_height {
                let desired_end = (entry_idx + half + 1).min(total);
                self.scroll_offset = total - desired_end;
            }
            self.tail_mode = false;
            // Select the line within the visible area.
            let (start, _end) = self.visible_range();
            self.selected_line = Some(entry_idx.saturating_sub(start));
        }
    }

    /// Format a single log entry as a styled [`Line`], with search highlighting.
    fn format_entry(&self, entry: &LogEntry, is_selected: bool) -> Line<'_> {
        let mut spans = Vec::new();

        // Selection indicator.
        if is_selected {
            spans.push(Span::styled(
                "► ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        }

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

        // Log line — with search term highlighting.
        if let Some(ref term) = self.search_term {
            spans.extend(highlight_matches(&entry.line, term, &self.theme));
        } else {
            spans.push(Span::raw(entry.line.clone()));
        }

        Line::from(spans)
    }
}

impl Component for LogViewer {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(sel) = self.selected_line {
                    let (start, end) = self.visible_range();
                    let max = (end - start).saturating_sub(1);
                    if sel < max {
                        self.selected_line = Some(sel + 1);
                    } else {
                        self.scroll_down(1);
                    }
                } else {
                    self.scroll_down(1);
                }
                Some(Action::Noop)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(sel) = self.selected_line {
                    if sel > 0 {
                        self.selected_line = Some(sel - 1);
                    } else {
                        self.scroll_up(1);
                    }
                } else {
                    self.scroll_up(1);
                }
                Some(Action::Noop)
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.scroll_to_top();
                self.selected_line = Some(0);
                Some(Action::Noop)
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.scroll_to_bottom();
                self.selected_line = None;
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
            // Toggle selection mode.
            KeyCode::Char(' ') => {
                if self.selected_line.is_some() {
                    self.selected_line = None;
                } else {
                    self.selected_line = Some(0);
                }
                Some(Action::Noop)
            }
            // Open detail view for selected line.
            KeyCode::Enter => Some(Action::ToggleDetail),
            // Search navigation.
            KeyCode::Char('n') => Some(Action::SearchNext),
            KeyCode::Char('N') => Some(Action::SearchPrev),
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
            Action::SearchSubmit(term) => {
                self.set_search(term.clone());
                None
            }
            Action::SearchNext => {
                self.search_next();
                None
            }
            Action::SearchPrev => {
                self.search_prev();
                None
            }
            Action::SearchClear => {
                self.clear_search();
                None
            }
            _ => None,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let inner_height = area.height.saturating_sub(2) as usize;

        // Build the title with mode indicators.
        let mut title_parts = vec![" Logs".to_string()];
        if self.tail_mode {
            title_parts.push("(TAIL)".to_string());
        } else if self.new_entries_count > 0 {
            title_parts.push(format!("(+{} new)", self.new_entries_count));
        }
        if let Some(ref term) = self.search_term {
            if self.search_matches.is_empty() {
                title_parts.push(format!("[/{term}: no matches]"));
            } else {
                title_parts.push(format!(
                    "[/{term}: {}/{}]",
                    self.search_match_cursor + 1,
                    self.search_matches.len()
                ));
            }
        }
        let title = format!("{} ", title_parts.join(" "));

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
        let (start, end) = self.visible_range();

        let lines: Vec<Line> = self.entries[start..end]
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let is_selected = self.selected_line == Some(i);
                self.format_entry(entry, is_selected)
            })
            .collect();

        let mut paragraph = Paragraph::new(lines).block(block);
        if self.line_wrap {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }

        frame.render_widget(paragraph, area);

        let _ = inner_height;
    }
}

/// Split a string by a search term and return styled spans with highlights.
fn highlight_matches<'a>(text: &str, term: &str, _theme: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let text_lower = text.to_lowercase();
    let term_lower = term.to_lowercase();
    let mut last_end = 0;

    for (start, _) in text_lower.match_indices(&term_lower) {
        // Text before the match.
        if start > last_end {
            spans.push(Span::raw(text[last_end..start].to_string()));
        }
        // The matched text (preserve original casing).
        let matched = &text[start..start + term.len()];
        spans.push(Span::styled(
            matched.to_string(),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        last_end = start + term.len();
    }

    // Remaining text after last match.
    if last_end < text.len() {
        spans.push(Span::raw(text[last_end..].to_string()));
    }

    if spans.is_empty() {
        spans.push(Span::raw(text.to_string()));
    }

    spans
}
