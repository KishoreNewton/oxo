//! Autocomplete dropdown popup widget.
//!
//! A reusable floating popup that displays a filtered list of suggestions
//! below the query bar. Used for label name and label value completion
//! when composing LogQL queries.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::theme::Theme;

/// Maximum number of visible items in the dropdown at once.
const MAX_VISIBLE: usize = 8;

/// A floating autocomplete popup showing filtered suggestions.
pub struct AutocompletePopup {
    /// Full, unfiltered suggestion list.
    items: Vec<String>,
    /// Items that match the current filter.
    filtered: Vec<String>,
    /// Current filter string.
    filter: String,
    /// Index of the currently highlighted item within `filtered`.
    selected: usize,
    /// Whether the popup is currently shown.
    visible: bool,
    /// Color theme.
    theme: Theme,
}

impl AutocompletePopup {
    /// Create a new autocomplete popup.
    pub fn new(theme: Theme) -> Self {
        Self {
            items: Vec::new(),
            filtered: Vec::new(),
            filter: String::new(),
            selected: 0,
            visible: false,
            theme,
        }
    }

    /// Set the full list of suggestions (unfiltered).
    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        self.rebuild_filtered();
    }

    /// Update the filter string and rebuild the filtered list.
    ///
    /// Uses case-insensitive matching: items that start with the filter are
    /// listed first, followed by items that contain the filter elsewhere.
    pub fn set_filter(&mut self, filter: &str) {
        self.filter = filter.to_string();
        self.rebuild_filtered();
    }

    /// Show the popup.
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the popup and reset selection.
    pub fn hide(&mut self) {
        self.visible = false;
        self.selected = 0;
    }

    /// Whether the popup is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Move the selection down by one.
    pub fn next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    /// Move the selection up by one.
    pub fn prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = if self.selected == 0 {
                self.filtered.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    /// Get the currently selected suggestion, if any.
    pub fn selected_item(&self) -> Option<&str> {
        self.filtered.get(self.selected).map(|s| s.as_str())
    }

    /// Render the autocomplete popup as a floating dropdown below `area`.
    ///
    /// `area` should be the rect of the query bar — the popup is positioned
    /// immediately below it.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible || self.filtered.is_empty() {
            return;
        }

        let frame_size = frame.area();

        // Compute popup dimensions.
        let item_count = self.filtered.len().min(MAX_VISIBLE);
        // +2 for top/bottom border, +1 for the match-count footer line.
        let popup_height =
            (item_count as u16 + 3).min(frame_size.height.saturating_sub(area.bottom()));
        let longest = self.filtered.iter().map(|s| s.len()).max().unwrap_or(0);
        // +4 for border + padding on each side.
        let popup_width = (longest as u16 + 4).max(20).min(area.width);

        // Position popup directly below the query bar, left-aligned.
        let popup_x = area.x;
        let popup_y = area.bottom();

        // Make sure we don't overflow the terminal.
        if popup_y >= frame_size.height || popup_height < 3 {
            return;
        }

        let popup_area = Rect::new(
            popup_x,
            popup_y,
            popup_width.min(frame_size.width.saturating_sub(popup_x)),
            popup_height.min(frame_size.height.saturating_sub(popup_y)),
        );

        // Clear background behind the popup.
        frame.render_widget(Clear, popup_area);

        // Determine the visible window of items (scrolling).
        let visible_count = (popup_area.height.saturating_sub(3)) as usize; // border top + border bottom + footer
        let scroll_offset = if self.selected >= visible_count {
            self.selected - visible_count + 1
        } else {
            0
        };

        let accent = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let normal = Style::default().fg(self.theme.fg);
        let dim = Style::default().fg(self.theme.fg_dim);

        let mut lines: Vec<Line<'_>> = Vec::with_capacity(visible_count + 1);

        for (i, item) in self
            .filtered
            .iter()
            .skip(scroll_offset)
            .take(visible_count)
            .enumerate()
        {
            let global_idx = i + scroll_offset;
            let style = if global_idx == self.selected {
                accent
            } else {
                normal
            };
            let prefix = if global_idx == self.selected {
                "► "
            } else {
                "  "
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(item.as_str(), style),
            ]));
        }

        // Footer with match count.
        let footer = format!(" {}/{} ", self.filtered.len(), self.items.len());
        lines.push(Line::from(Span::styled(footer, dim)));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }

    /// Rebuild the filtered list from current items and filter.
    fn rebuild_filtered(&mut self) {
        let filter_lower = self.filter.to_lowercase();

        if filter_lower.is_empty() {
            self.filtered = self.items.clone();
        } else {
            // Prefix matches first, then contains matches.
            let mut prefix_matches = Vec::new();
            let mut contains_matches = Vec::new();

            for item in &self.items {
                let item_lower = item.to_lowercase();
                if item_lower.starts_with(&filter_lower) {
                    prefix_matches.push(item.clone());
                } else if item_lower.contains(&filter_lower) {
                    contains_matches.push(item.clone());
                }
            }

            prefix_matches.extend(contains_matches);
            self.filtered = prefix_matches;
        }

        // Keep selection in bounds.
        if self.filtered.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.filtered.len() - 1);
        }
    }
}
