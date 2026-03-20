//! The main log viewer component.
//!
//! Displays a scrollable list of log entries with:
//! - Timestamp coloring and log-level highlighting
//! - Auto-scroll (tail mode) with "N new lines" indicator
//! - Live search with match highlighting and n/N navigation
//! - Line selection for detail/inspect view
//! - Mouse scroll support

use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};
use regex::Regex;

use oxo_core::LogEntry;
use oxo_core::multiline::{self, GroupedEntry};
use oxo_core::structured::StructuredData;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Deduplication mode — cycles Off → Exact → Fuzzy → Off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DedupMode {
    /// No deduplication.
    Off,
    /// Group all entries with identical `.line` content (global, not just consecutive).
    Exact,
    /// Normalize lines by stripping variable tokens (UUIDs, IPs, timestamps,
    /// hex strings, numbers, request IDs) then group by normalized form.
    Fuzzy,
}

impl DedupMode {
    /// Cycle to the next mode.
    fn next(self) -> Self {
        match self {
            Self::Off => Self::Exact,
            Self::Exact => Self::Fuzzy,
            Self::Fuzzy => Self::Off,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::Exact => "DEDUP:EXACT",
            Self::Fuzzy => "DEDUP:FUZZY",
        }
    }
}

/// A group of deduplicated log entries.
struct DedupGroup {
    /// Index of the representative (most recent) entry in the entries buffer.
    entry_idx: usize,
    /// All entry indices belonging to this group (used for expansion).
    #[allow(dead_code)]
    member_indices: Vec<usize>,
    /// Number of entries in this group.
    count: usize,
    /// Timestamp of the first entry in the group.
    first_timestamp: DateTime<Utc>,
    /// Timestamp of the last entry in the group.
    last_timestamp: DateTime<Utc>,
}

/// Lazy-compiled regexes for fuzzy normalization.
struct NormPatterns {
    uuid: Regex,
    ipv4: Regex,
    hex_token: Regex,
    iso_timestamp: Regex,
    duration: Regex,
    number: Regex,
}

impl NormPatterns {
    fn new() -> Self {
        Self {
            uuid: Regex::new(
                r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
            )
            .unwrap(),
            ipv4: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(:\d+)?\b").unwrap(),
            hex_token: Regex::new(r"\b[0-9a-fA-F]{16,}\b").unwrap(),
            iso_timestamp: Regex::new(
                r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?",
            )
            .unwrap(),
            duration: Regex::new(r"\b\d+(\.\d+)?\s*(ms|us|µs|ns|s|sec|min)\b").unwrap(),
            number: Regex::new(r"\b\d{2,}\b").unwrap(),
        }
    }

    /// Normalize a log line by replacing variable tokens with placeholders.
    fn normalize(&self, line: &str) -> String {
        let s = self.uuid.replace_all(line, "<UUID>");
        let s = self.ipv4.replace_all(&s, "<IP>");
        let s = self.hex_token.replace_all(&s, "<HEX>");
        let s = self.iso_timestamp.replace_all(&s, "<TS>");
        let s = self.duration.replace_all(&s, "<DUR>");
        let s = self.number.replace_all(&s, "<N>");
        s.into_owned()
    }
}

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

    /// Compiled regex for the active search term.
    search_regex: Option<Regex>,

    /// Indices of entries matching the search term.
    search_matches: Vec<usize>,

    /// Current match index within search_matches.
    search_match_cursor: usize,

    /// The color theme.
    theme: Theme,

    /// Grouped entries (multi-line log groups with continuation lines).
    grouped_entries: Vec<GroupedEntry>,

    /// Number of context lines to show around search matches (0 = off).
    context_lines: usize,

    // ── Column mode ──────────────────────────────────────────────────
    /// Whether column/table mode is active.
    column_mode: bool,

    /// Discovered column names from structured log data.
    discovered_columns: Vec<String>,

    /// Active sort column and direction: (column_index, ascending).
    sort_column: Option<(usize, bool)>,

    // ── Deduplication ──────────────────────────────────────────────
    /// Current dedup mode (Off / Exact / Fuzzy).
    dedup_mode: DedupMode,

    /// Groups of deduplicated entries.
    dedup_groups: Vec<DedupGroup>,

    /// Lazy-compiled normalization patterns for fuzzy dedup.
    norm_patterns: Option<NormPatterns>,

    // ── Bookmarks ──────────────────────────────────────────────────
    /// Set of bookmarked entry indices.
    bookmarks: HashSet<usize>,
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
            search_regex: None,
            search_matches: Vec::new(),
            search_match_cursor: 0,
            theme,
            grouped_entries: Vec::new(),
            context_lines: 0,
            column_mode: false,
            discovered_columns: Vec::new(),
            sort_column: None,
            dedup_mode: DedupMode::Off,
            dedup_groups: Vec::new(),
            norm_patterns: None,
            bookmarks: HashSet::new(),
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

        // Bookmarks store buffer indices which become invalid when entries
        // shift. Clear them on every update to avoid pointing at wrong lines.
        if !self.bookmarks.is_empty() && self.entries.len() != previous_len {
            self.bookmarks.clear();
        }

        // Rebuild search matches if there's an active search.
        if self.search_term.is_some() {
            self.rebuild_search_matches();
        }

        // Rebuild multi-line groups.
        self.grouped_entries = multiline::group_entries(&self.entries);

        // Rebuild dedup groups if dedup is active.
        if self.dedup_mode != DedupMode::Off {
            self.rebuild_dedup_groups();
        }

        // Re-discover columns if column mode is active.
        if self.column_mode {
            self.discover_columns();
        }
    }

    /// Set the viewport height (called by App after layout is computed).
    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height;
    }

    /// Whether the viewer is currently in tail (auto-scroll) mode.
    pub fn is_tail_mode(&self) -> bool {
        self.tail_mode
    }

    /// Select a specific line by index within the visible area.
    ///
    /// If the index is out of range, the selection is cleared.
    pub fn select_line(&mut self, visible_index: usize) {
        let (start, end) = self.visible_range();
        let visible_count = end.saturating_sub(start);
        if visible_index < visible_count {
            self.selected_line = Some(visible_index);
        }
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
        // Try to compile the term as a case-insensitive regex.
        // On failure, escape it so it is treated as a literal pattern.
        let pattern = format!("(?i){term}");
        let compiled = Regex::new(&pattern).unwrap_or_else(|_| {
            Regex::new(&format!("(?i){}", regex::escape(&term)))
                .expect("escaped literal regex must always compile")
        });
        self.search_term = Some(term);
        self.search_regex = Some(compiled);
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
        self.search_regex = None;
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
        if let Some(ref re) = self.search_regex {
            for (i, entry) in self.entries.iter().enumerate() {
                if re.is_match(&entry.line) {
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

    /// Toggle expand/collapse of the selected multi-line group.
    pub fn toggle_expand(&mut self) {
        if let Some(sel) = self.selected_line {
            let (start, _end) = self.visible_range();
            let entry_idx = start + sel;

            // Find which grouped entry owns this entry index.
            if let Some(group_idx) = self.entry_index_to_group(entry_idx) {
                self.grouped_entries[group_idx].collapsed =
                    !self.grouped_entries[group_idx].collapsed;
            }
        }
    }

    /// Cycle the search context lines setting: 0 → 3 → 5 → 10 → 0.
    pub fn toggle_context(&mut self) {
        self.context_lines = match self.context_lines {
            0 => 3,
            3 => 5,
            5 => 10,
            _ => 0,
        };
    }

    /// Get the current context lines setting.
    pub fn context_lines(&self) -> usize {
        self.context_lines
    }

    /// Map a flat entry index to the index of the [`GroupedEntry`] that owns it.
    ///
    /// Each `GroupedEntry` accounts for 1 entry (the parent) plus its
    /// continuation lines. We walk through the groups, accumulating the count
    /// of original entries consumed, until we find the group containing
    /// `entry_idx`.
    fn entry_index_to_group(&self, entry_idx: usize) -> Option<usize> {
        let mut consumed = 0usize;
        for (gi, group) in self.grouped_entries.iter().enumerate() {
            let group_size = 1 + group.continuation_lines.len();
            if entry_idx < consumed + group_size {
                return Some(gi);
            }
            consumed += group_size;
        }
        None
    }

    /// Find the grouped entry for a given flat entry index, if any, and return
    /// it along with the flat index of the group's parent entry.
    fn group_for_entry(&self, entry_idx: usize) -> Option<(usize, &GroupedEntry)> {
        let mut consumed = 0usize;
        for group in self.grouped_entries.iter() {
            let group_size = 1 + group.continuation_lines.len();
            if entry_idx < consumed + group_size {
                return Some((consumed, group));
            }
            consumed += group_size;
        }
        None
    }

    /// Build the set of entry indices that should be shown as context around
    /// search matches. Returns a set of indices that are context (not the
    /// match itself).
    fn build_context_set(&self) -> std::collections::HashSet<usize> {
        let mut ctx = std::collections::HashSet::new();
        if self.context_lines == 0 || self.search_matches.is_empty() {
            return ctx;
        }
        let total = self.entries.len();
        for &match_idx in &self.search_matches {
            let start = match_idx.saturating_sub(self.context_lines);
            let end = (match_idx + self.context_lines + 1).min(total);
            for i in start..end {
                if i != match_idx {
                    ctx.insert(i);
                }
            }
        }
        ctx
    }

    /// Check whether a given entry index falls between two context groups
    /// (i.e., there is a gap where a separator should be shown).
    fn is_gap_before(&self, entry_idx: usize) -> bool {
        if self.context_lines == 0 || self.search_matches.is_empty() {
            return false;
        }
        // A gap exists if the previous entry is NOT a match and NOT context.
        if entry_idx == 0 {
            return false;
        }
        let match_set: std::collections::HashSet<usize> =
            self.search_matches.iter().copied().collect();
        let ctx_set = self.build_context_set();
        let prev = entry_idx - 1;
        let this_is_relevant = match_set.contains(&entry_idx) || ctx_set.contains(&entry_idx);
        let prev_is_relevant = match_set.contains(&prev) || ctx_set.contains(&prev);
        this_is_relevant && !prev_is_relevant
    }

    /// Format a single log entry as a styled [`Line`], with search highlighting.
    ///
    /// The entire log line text is tinted with the level color (if a known
    /// level label is present). Search highlights override the level tint so
    /// matches always stand out.
    fn format_entry(&self, entry: &LogEntry, entry_idx: usize, is_selected: bool) -> Line<'_> {
        let mut spans = Vec::new();

        // Determine the level-based style for the entire line body.
        let level_str = entry.labels.get("level").or(entry.labels.get("severity"));
        let mut line_style = level_str
            .and_then(|l| self.theme.log_level_color(l))
            .map(|c| Style::default().fg(c))
            .unwrap_or_default();
        // Fatal / critical also gets bold on the whole line.
        if let Some(l) = level_str {
            if matches!(l.to_lowercase().as_str(), "fatal" | "critical") {
                line_style = line_style.add_modifier(Modifier::BOLD);
            }
        }

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

        // Log level prefix (if present in labels).
        if let Some(level) = level_str {
            let style = self.theme.log_level_style(level);
            spans.push(Span::styled(format!("[{level:>5}]"), style));
            spans.push(Span::raw(" "));
        }

        // Log line body — search highlights override level color.
        if let Some(ref re) = self.search_regex {
            spans.extend(highlight_matches(&entry.line, re, &self.theme, line_style));
        } else {
            spans.push(Span::styled(entry.line.clone(), line_style));
        }

        // If this entry is the parent of a collapsed multi-line group, add
        // a collapse indicator showing the number of hidden continuation lines.
        if let Some((parent_idx, group)) = self.group_for_entry(entry_idx) {
            // Only annotate the parent line (not continuation lines).
            if entry_idx == parent_idx && !group.continuation_lines.is_empty() && group.collapsed {
                let count = group.continuation_lines.len();
                spans.push(Span::styled(
                    format!(" \u{25B6} +{count} lines"),
                    self.theme.dimmed(),
                ));
            }
        }

        Line::from(spans)
    }

    /// Format a continuation line with a dim `│ ` prefix.
    fn format_continuation_line(&self, text: &str) -> Line<'_> {
        let spans = vec![
            Span::styled("│ ", self.theme.dimmed()),
            Span::styled(text.to_string(), self.theme.dimmed()),
        ];
        Line::from(spans)
    }

    /// Format a context entry (dimmed with `·` prefix instead of line number).
    fn format_context_entry(&self, entry: &LogEntry) -> Line<'_> {
        let mut spans = vec![Span::styled("· ", self.theme.dimmed())];

        if self.show_timestamps {
            let ts = entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
            spans.push(Span::styled(ts, self.theme.dimmed()));
            spans.push(Span::raw(" "));
        }

        let level_str = entry.labels.get("level").or(entry.labels.get("severity"));
        if let Some(level) = level_str {
            spans.push(Span::styled(format!("[{level:>5}]"), self.theme.dimmed()));
            spans.push(Span::raw(" "));
        }

        spans.push(Span::styled(entry.line.clone(), self.theme.dimmed()));
        Line::from(spans)
    }

    /// Create a separator line (`---`) used between context groups.
    fn format_separator(&self) -> Line<'_> {
        Line::from(Span::styled("---", self.theme.dimmed()))
    }

    // ── Column mode methods ─────────────────────────────────────────

    /// Scan visible entries, parse structured data, and collect all unique
    /// field names. Columns are ordered: timestamp first (if present), then
    /// level, then the rest alphabetically.
    fn discover_columns(&mut self) {
        let mut field_set: HashSet<String> = HashSet::new();

        let (start, end) = self.visible_range();
        for entry in &self.entries[start..end] {
            if let Some(sd) = StructuredData::parse(&entry.line) {
                for (key, _) in sd.fields() {
                    field_set.insert(key);
                }
            }
        }

        let mut priority_cols: Vec<String> = Vec::new();
        let mut rest: Vec<String> = Vec::new();

        // "timestamp" first (if present).
        if field_set.remove("timestamp") {
            priority_cols.push("timestamp".to_string());
        }
        // "level" second (if present).
        if field_set.remove("level") {
            priority_cols.push("level".to_string());
        }

        // Everything else, sorted alphabetically.
        rest.extend(field_set);
        rest.sort();

        priority_cols.extend(rest);
        self.discovered_columns = priority_cols;
    }

    /// Toggle column/table mode on or off. Discovers columns when enabling.
    pub fn toggle_column_mode(&mut self) {
        self.column_mode = !self.column_mode;
        if self.column_mode {
            self.discover_columns();
        }
    }

    /// Sort entries by the given column index. Toggles direction if the
    /// same column is selected again; otherwise sets ascending.
    pub fn sort_by_column(&mut self, col_idx: usize) {
        if col_idx >= self.discovered_columns.len() {
            return;
        }
        let ascending = match self.sort_column {
            Some((prev_idx, prev_asc)) if prev_idx == col_idx => !prev_asc,
            _ => true,
        };
        self.sort_column = Some((col_idx, ascending));

        let col_name = self.discovered_columns[col_idx].clone();
        self.entries.sort_by(|a, b| {
            let val_a = StructuredData::parse(&a.line)
                .and_then(|sd| sd.get(&col_name))
                .unwrap_or_default();
            let val_b = StructuredData::parse(&b.line)
                .and_then(|sd| sd.get(&col_name))
                .unwrap_or_default();
            if ascending {
                val_a.cmp(&val_b)
            } else {
                val_b.cmp(&val_a)
            }
        });

        // Sorting reorders entries, invalidating selection and dedup groups.
        self.selected_line = None;
        self.bookmarks.clear();
        if self.dedup_mode != DedupMode::Off {
            self.rebuild_dedup_groups();
        }
    }

    // ── Dedup methods ───────────────────────────────────────────────

    /// Rebuild dedup groups based on the current dedup mode.
    fn rebuild_dedup_groups(&mut self) {
        self.dedup_groups.clear();
        if self.entries.is_empty() {
            return;
        }

        match self.dedup_mode {
            DedupMode::Off => {}
            DedupMode::Exact => self.build_exact_dedup_groups(),
            DedupMode::Fuzzy => self.build_fuzzy_dedup_groups(),
        }

        // Sort groups by the most recent entry timestamp (descending)
        // so the viewer shows the most recently seen patterns first.
        self.dedup_groups
            .sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
    }

    /// Exact dedup: group all entries with identical `.line` content globally.
    fn build_exact_dedup_groups(&mut self) {
        let mut groups: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, entry) in self.entries.iter().enumerate() {
            groups.entry(entry.line.as_str()).or_default().push(i);
        }

        for (_line, indices) in groups {
            let first_idx = indices[0];
            let last_idx = *indices.last().unwrap();
            self.dedup_groups.push(DedupGroup {
                entry_idx: last_idx, // most recent as representative
                member_indices: indices.clone(),
                count: indices.len(),
                first_timestamp: self.entries[first_idx].timestamp,
                last_timestamp: self.entries[last_idx].timestamp,
            });
        }
    }

    /// Fuzzy dedup: normalize lines then group by normalized form.
    fn build_fuzzy_dedup_groups(&mut self) {
        // Lazily initialize normalization patterns.
        if self.norm_patterns.is_none() {
            self.norm_patterns = Some(NormPatterns::new());
        }
        let patterns = self.norm_patterns.as_ref().unwrap();

        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, entry) in self.entries.iter().enumerate() {
            let key = patterns.normalize(&entry.line);
            groups.entry(key).or_default().push(i);
        }

        for (_key, indices) in groups {
            let first_idx = indices[0];
            let last_idx = *indices.last().unwrap();
            self.dedup_groups.push(DedupGroup {
                entry_idx: last_idx,
                member_indices: indices.clone(),
                count: indices.len(),
                first_timestamp: self.entries[first_idx].timestamp,
                last_timestamp: self.entries[last_idx].timestamp,
            });
        }
    }

    /// Cycle dedup mode: Off → Exact → Fuzzy → Off.
    pub fn toggle_dedup(&mut self) {
        self.dedup_mode = self.dedup_mode.next();
        if self.dedup_mode != DedupMode::Off {
            self.rebuild_dedup_groups();
        } else {
            self.dedup_groups.clear();
        }
    }

    // ── Bookmark methods ────────────────────────────────────────────

    /// Toggle a bookmark on the currently selected entry.
    pub fn toggle_bookmark(&mut self) {
        if let Some(sel) = self.selected_line {
            let (start, _end) = self.visible_range();
            let entry_idx = start + sel;
            if entry_idx < self.entries.len() && !self.bookmarks.remove(&entry_idx) {
                self.bookmarks.insert(entry_idx);
            }
        }
    }

    /// Jump to the next bookmarked entry after the current selection.
    pub fn next_bookmark(&mut self) {
        if self.bookmarks.is_empty() {
            return;
        }
        let (start, _end) = self.visible_range();
        // When no line is selected, use MAX so the wrap-around lands on the
        // first bookmark (via `.or(sorted.first())`).
        let current = self
            .selected_line
            .map(|sel| start + sel)
            .unwrap_or(usize::MAX);

        // Find the smallest bookmark index > current.
        let mut sorted: Vec<usize> = self.bookmarks.iter().copied().collect();
        sorted.sort();

        let next = sorted
            .iter()
            .find(|&&idx| idx > current)
            .or(sorted.first())
            .copied();

        if let Some(idx) = next {
            self.scroll_to_entry(idx);
        }
    }

    /// Jump to the previous bookmarked entry before the current selection.
    pub fn prev_bookmark(&mut self) {
        if self.bookmarks.is_empty() {
            return;
        }
        let (start, _end) = self.visible_range();
        let current = self.selected_line.map(|sel| start + sel).unwrap_or(0);

        let mut sorted: Vec<usize> = self.bookmarks.iter().copied().collect();
        sorted.sort();

        let prev = sorted
            .iter()
            .rev()
            .find(|&&idx| idx < current)
            .or(sorted.last())
            .copied();

        if let Some(idx) = prev {
            self.scroll_to_entry(idx);
        }
    }

    /// Clear all bookmarks.
    pub fn clear_bookmarks(&mut self) {
        self.bookmarks.clear();
    }

    /// Scroll the view so that the given entry index is visible and selected.
    fn scroll_to_entry(&mut self, entry_idx: usize) {
        let total = self.entries.len();
        if entry_idx >= total {
            return;
        }
        let half = self.viewport_height / 2;
        if total > self.viewport_height {
            let desired_end = (entry_idx + half + 1).min(total);
            self.scroll_offset = total - desired_end;
        }
        self.tail_mode = false;
        let (start, _end) = self.visible_range();
        self.selected_line = Some(entry_idx.saturating_sub(start));
    }

    /// Format an entry with a bookmark marker prepended if bookmarked.
    fn format_entry_with_bookmark(
        &self,
        entry: &LogEntry,
        entry_idx: usize,
        is_selected: bool,
    ) -> Line<'_> {
        let mut line = self.format_entry(entry, entry_idx, is_selected);
        if self.bookmarks.contains(&entry_idx) {
            // Prepend the bookmark marker at the very beginning.
            line.spans.insert(
                0,
                Span::styled(
                    "\u{25C6} ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            );
        }
        line
    }

    /// Render the column/table mode view using a [`Table`] widget.
    fn render_column_mode(&self, frame: &mut Frame, area: Rect, block: Block<'_>) {
        let (start, end) = self.visible_range();
        let col_count = self.discovered_columns.len();

        // Build the header row.
        let header_cells: Vec<Cell> = self
            .discovered_columns
            .iter()
            .enumerate()
            .map(|(ci, name)| {
                let mut label = name.clone();
                if let Some((sort_idx, ascending)) = self.sort_column {
                    if sort_idx == ci {
                        label.push_str(if ascending { " \u{25B2}" } else { " \u{25BC}" });
                    }
                }
                Cell::from(label).style(
                    Style::default()
                        .fg(self.theme.fg)
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect();
        let header = Row::new(header_cells)
            .style(Style::default().fg(self.theme.fg))
            .bottom_margin(0);

        // Build data rows.
        let mut rows: Vec<Row> = Vec::new();
        for (i, entry) in self.entries[start..end].iter().enumerate() {
            let entry_idx = start + i;
            let is_selected = self.selected_line == Some(i);
            let is_bookmarked = self.bookmarks.contains(&entry_idx);

            let parsed = StructuredData::parse(&entry.line);

            let cells: Vec<Cell> = self
                .discovered_columns
                .iter()
                .enumerate()
                .map(|(ci, col_name)| {
                    let value = parsed
                        .as_ref()
                        .and_then(|sd| sd.get(col_name))
                        .unwrap_or_else(|| "-".to_string());

                    // For the first column, optionally prepend the bookmark marker.
                    let display = if ci == 0 && is_bookmarked {
                        format!("\u{25C6} {value}")
                    } else {
                        value
                    };

                    Cell::from(display)
                })
                .collect();

            let row_style = if is_selected {
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                // Tint by log level if available.
                let level_str = entry.labels.get("level").or(entry.labels.get("severity"));
                level_str
                    .and_then(|l| self.theme.log_level_color(l))
                    .map(|c| Style::default().fg(c))
                    .unwrap_or_default()
            };

            rows.push(Row::new(cells).style(row_style));
        }

        // Compute column widths: distribute evenly with a minimum width.
        let widths: Vec<Constraint> = if col_count > 0 {
            let pct = 100u16.saturating_div(col_count as u16).max(1);
            (0..col_count)
                .map(|_| Constraint::Min(pct.max(8)))
                .collect()
        } else {
            vec![Constraint::Percentage(100)]
        };

        let table = Table::new(rows, &widths)
            .header(header)
            .block(block)
            .column_spacing(1);

        frame.render_widget(table, area);
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
            Action::ScrollUp(n) => {
                self.scroll_up(*n);
                None
            }
            Action::ScrollDown(n) => {
                self.scroll_down(*n);
                None
            }
            Action::SelectLine(idx) => {
                self.select_line(*idx);
                None
            }
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
            Action::ToggleExpand => {
                self.toggle_expand();
                None
            }
            Action::ToggleContext => {
                self.toggle_context();
                None
            }
            Action::ToggleColumnMode => {
                self.toggle_column_mode();
                None
            }
            Action::SortColumn(idx) => {
                self.sort_by_column(*idx);
                None
            }
            Action::ToggleDedup => {
                self.toggle_dedup();
                None
            }
            Action::ToggleBookmark => {
                self.toggle_bookmark();
                None
            }
            Action::NextBookmark => {
                self.next_bookmark();
                None
            }
            Action::PrevBookmark => {
                self.prev_bookmark();
                None
            }
            Action::ClearBookmarks => {
                self.clear_bookmarks();
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
        if self.context_lines > 0 {
            title_parts.push(format!("[ctx:{}]", self.context_lines));
        }
        if self.column_mode {
            title_parts.push("[COL]".to_string());
        }
        if self.dedup_mode != DedupMode::Off {
            let total = self.entries.len();
            let groups = self.dedup_groups.len();
            title_parts.push(format!(
                "[{} {}→{}]",
                self.dedup_mode.label(),
                total,
                groups
            ));
        }
        if !self.bookmarks.is_empty() {
            title_parts.push(format!("[{}bm]", self.bookmarks.len()));
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

        // ── Column mode rendering ───────────────────────────────────
        if self.column_mode && !self.discovered_columns.is_empty() {
            self.render_column_mode(frame, area, block);
            let _ = inner_height;
            return;
        }

        // Get the visible slice of entries.
        let (start, end) = self.visible_range();

        // ── Dedup mode rendering ────────────────────────────────────
        if self.dedup_mode != DedupMode::Off && !self.dedup_groups.is_empty() {
            let mut lines: Vec<Line> = Vec::new();
            let total_groups = self.dedup_groups.len();

            // Pagination: use scroll_offset to paginate through dedup groups.
            let visible_end = total_groups.saturating_sub(self.scroll_offset);
            let visible_start = visible_end.saturating_sub(inner_height);

            for (gi, group) in self.dedup_groups[visible_start..visible_end]
                .iter()
                .enumerate()
            {
                let is_selected = self.selected_line == Some(gi);
                let entry = &self.entries[group.entry_idx];

                let mut line = self.format_entry_with_bookmark(entry, group.entry_idx, is_selected);

                if group.count > 1 {
                    // Show count badge.
                    line.spans.push(Span::styled(
                        format!(" (x{})", group.count),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));

                    // For fuzzy mode, show time span if entries span > 1 second.
                    if self.dedup_mode == DedupMode::Fuzzy {
                        let span_secs = (group.last_timestamp - group.first_timestamp)
                            .num_seconds()
                            .abs();
                        if span_secs > 0 {
                            let span_str = if span_secs >= 3600 {
                                format!(" over {}h", span_secs / 3600)
                            } else if span_secs >= 60 {
                                format!(" over {}m", span_secs / 60)
                            } else {
                                format!(" over {}s", span_secs)
                            };
                            line.spans.push(Span::styled(span_str, self.theme.dimmed()));
                        }
                    }
                }
                lines.push(line);
            }

            let mut paragraph = Paragraph::new(lines).block(block);
            if self.line_wrap {
                paragraph = paragraph.wrap(Wrap { trim: false });
            }
            frame.render_widget(paragraph, area);
            return;
        }

        // ── Normal mode rendering ───────────────────────────────────
        // Build context information for context view mode.
        let search_active = self.search_term.is_some() && self.context_lines > 0;
        let match_set: HashSet<usize> = if search_active {
            self.search_matches.iter().copied().collect()
        } else {
            HashSet::new()
        };
        let ctx_set = if search_active {
            self.build_context_set()
        } else {
            HashSet::new()
        };

        let mut lines: Vec<Line> = Vec::new();

        for (i, entry) in self.entries[start..end].iter().enumerate() {
            let entry_idx = start + i;
            let is_selected = self.selected_line == Some(i);

            // Context view filtering: when context mode is active and search
            // is active, only show matches, context entries, and separators.
            if search_active {
                let is_match = match_set.contains(&entry_idx);
                let is_context = ctx_set.contains(&entry_idx);

                if !is_match && !is_context {
                    continue;
                }

                // Insert separator if there is a gap before this entry.
                if self.is_gap_before(entry_idx) {
                    lines.push(self.format_separator());
                }

                if is_context && !is_match {
                    lines.push(self.format_context_entry(entry));
                } else {
                    lines.push(self.format_entry_with_bookmark(entry, entry_idx, is_selected));
                }
            } else {
                // Check if this entry is a continuation line within a collapsed group.
                if let Some((parent_idx, group)) = self.group_for_entry(entry_idx) {
                    if entry_idx != parent_idx && group.collapsed {
                        // Skip continuation lines when collapsed (the count
                        // indicator is appended to the parent line by format_entry).
                        continue;
                    }
                    if entry_idx != parent_idx && !group.collapsed {
                        // Show expanded continuation line with │ prefix.
                        lines.push(self.format_continuation_line(&entry.line));
                        continue;
                    }
                }

                lines.push(self.format_entry_with_bookmark(entry, entry_idx, is_selected));
            }
        }

        let mut paragraph = Paragraph::new(lines).block(block);
        if self.line_wrap {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }

        frame.render_widget(paragraph, area);

        let _ = inner_height;
    }
}

/// Split a string by a regex and return styled spans with highlights.
///
/// Non-matching segments use `base_style` (typically the level color), while
/// matching segments get the hard-coded search highlight (black on yellow,
/// bold), which completely overrides the base style so matches always pop.
fn highlight_matches<'a>(
    text: &str,
    regex: &Regex,
    _theme: &Theme,
    base_style: Style,
) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut last_end = 0;

    for m in regex.find_iter(text) {
        let start = m.start();
        let end = m.end();

        // Text before the match — use the level-based base style.
        if start > last_end {
            spans.push(Span::styled(text[last_end..start].to_string(), base_style));
        }
        // The matched text — search highlight overrides level color.
        spans.push(Span::styled(
            text[start..end].to_string(),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        last_end = end;
    }

    // Remaining text after last match.
    if last_end < text.len() {
        spans.push(Span::styled(text[last_end..].to_string(), base_style));
    }

    if spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base_style));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make_entry(line: &str, secs: i64) -> LogEntry {
        LogEntry {
            timestamp: DateTime::from_timestamp(secs, 0).unwrap(),
            labels: BTreeMap::new(),
            line: line.to_string(),
            raw: None,
        }
    }

    #[test]
    fn dedup_mode_cycles() {
        assert_eq!(DedupMode::Off.next(), DedupMode::Exact);
        assert_eq!(DedupMode::Exact.next(), DedupMode::Fuzzy);
        assert_eq!(DedupMode::Fuzzy.next(), DedupMode::Off);
    }

    #[test]
    fn exact_dedup_groups_identical_lines() {
        let theme = Theme::default();
        let mut viewer = LogViewer::new(theme);
        let entries: VecDeque<LogEntry> = vec![
            make_entry("error: connection refused", 1000),
            make_entry("info: request handled", 1001),
            make_entry("error: connection refused", 1002),
            make_entry("info: request handled", 1003),
            make_entry("error: connection refused", 1004),
        ]
        .into_iter()
        .collect();

        viewer.update_entries(&entries);
        viewer.dedup_mode = DedupMode::Exact;
        viewer.rebuild_dedup_groups();

        assert_eq!(viewer.dedup_groups.len(), 2);

        // Find the group for "error: connection refused"
        let error_group = viewer
            .dedup_groups
            .iter()
            .find(|g| viewer.entries[g.entry_idx].line == "error: connection refused")
            .unwrap();
        assert_eq!(error_group.count, 3);

        let info_group = viewer
            .dedup_groups
            .iter()
            .find(|g| viewer.entries[g.entry_idx].line == "info: request handled")
            .unwrap();
        assert_eq!(info_group.count, 2);
    }

    #[test]
    fn fuzzy_dedup_groups_similar_lines() {
        let theme = Theme::default();
        let mut viewer = LogViewer::new(theme);
        let entries: VecDeque<LogEntry> = vec![
            make_entry("error: connection to 10.0.0.1:5432 refused", 1000),
            make_entry("error: connection to 10.0.0.2:5432 refused", 1001),
            make_entry("info: something else", 1002),
            make_entry("error: connection to 192.168.1.1:5432 refused", 1003),
        ]
        .into_iter()
        .collect();

        viewer.update_entries(&entries);
        viewer.dedup_mode = DedupMode::Fuzzy;
        viewer.rebuild_dedup_groups();

        // The three "error: connection to <IP> refused" lines should group
        // together since IPs get normalized to <IP>.
        assert_eq!(viewer.dedup_groups.len(), 2);

        let error_group = viewer
            .dedup_groups
            .iter()
            .find(|g| g.count == 3)
            .expect("should have a group of 3 similar error lines");
        assert_eq!(error_group.first_timestamp.timestamp(), 1000);
        assert_eq!(error_group.last_timestamp.timestamp(), 1003);
    }

    #[test]
    fn fuzzy_normalizer_patterns() {
        let p = NormPatterns::new();

        // UUIDs
        assert_eq!(
            p.normalize("req 550e8400-e29b-41d4-a716-446655440000 failed"),
            "req <UUID> failed"
        );

        // IPs
        assert_eq!(
            p.normalize("connect to 192.168.1.100:3306"),
            "connect to <IP>"
        );

        // ISO timestamps
        assert_eq!(
            p.normalize("at 2024-01-15T10:30:00Z something"),
            "at <TS> something"
        );

        // Durations
        assert_eq!(
            p.normalize("took 432ms to complete"),
            "took <DUR> to complete"
        );

        // Numbers (2+ digits)
        assert_eq!(p.normalize("status 500 returned"), "status <N> returned");

        // Long hex tokens
        assert_eq!(
            p.normalize("trace_id=abcdef0123456789 span"),
            "trace_id=<HEX> span"
        );
    }

    #[test]
    fn toggle_dedup_cycles_through_modes() {
        let theme = Theme::default();
        let mut viewer = LogViewer::new(theme);
        assert_eq!(viewer.dedup_mode, DedupMode::Off);

        viewer.toggle_dedup();
        assert_eq!(viewer.dedup_mode, DedupMode::Exact);

        viewer.toggle_dedup();
        assert_eq!(viewer.dedup_mode, DedupMode::Fuzzy);

        viewer.toggle_dedup();
        assert_eq!(viewer.dedup_mode, DedupMode::Off);
        assert!(viewer.dedup_groups.is_empty());
    }
}
