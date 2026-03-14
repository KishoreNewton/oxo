//! Tab bar component.
//!
//! Displays a row of tabs above the log viewer, one per active query.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Maximum number of concurrent tabs.
pub const MAX_TABS: usize = 9;

/// A single tab's metadata.
#[derive(Debug, Clone)]
pub struct TabInfo {
    /// Short label shown in the tab bar.
    pub label: String,
    /// The full query for this tab.
    pub query: String,
}

/// Tab bar component.
pub struct TabBar {
    tabs: Vec<TabInfo>,
    active: usize,
    theme: Theme,
}

impl TabBar {
    /// Create a new tab bar with one default tab.
    pub fn new(theme: Theme) -> Self {
        let initial = TabInfo {
            label: "1: {}".to_string(),
            query: "{}".to_string(),
        };
        Self {
            tabs: vec![initial],
            active: 0,
            theme,
        }
    }

    /// Add a new tab. Returns its index, or `None` if at [`MAX_TABS`].
    pub fn add_tab(&mut self, query: String) -> Option<usize> {
        if self.tabs.len() >= MAX_TABS {
            return None;
        }
        let n = self.tabs.len() + 1; // 1-based display number
        let short_query: String = query.chars().take(20).collect();
        let label = format!("{n}: {short_query}");
        self.tabs.push(TabInfo { label, query });
        Some(self.tabs.len() - 1)
    }

    /// Remove the tab at `index` (no-op if it is the last tab).
    pub fn close_tab(&mut self, index: usize) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        // Re-number labels to keep them sequential.
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            let n = i + 1;
            let short_query: String = tab.query.chars().take(20).collect();
            tab.label = format!("{n}: {short_query}");
        }
        // Clamp active index.
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
    }

    /// Return the index of the currently active tab.
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// Switch the active tab to `index` (silently ignored if out of range).
    pub fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    /// Return a reference to the currently active [`TabInfo`].
    pub fn active_tab(&self) -> &TabInfo {
        &self.tabs[self.active]
    }

    /// Return the total number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Return a slice of all tabs.
    pub fn tabs(&self) -> &[TabInfo] {
        &self.tabs
    }
}

impl Component for TabBar {
    // Tab switching is handled at the app level via keybindings.
    fn handle_key(&mut self, _key: crossterm::event::KeyEvent) -> Option<Action> {
        None
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        let active_style = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let inactive_style = self.theme.dimmed();

        let titles: Vec<Line> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let style = if i == self.active {
                    active_style
                } else {
                    inactive_style
                };
                Line::from(Span::styled(tab.label.clone(), style))
            })
            .collect();

        let tabs_widget = Tabs::new(titles)
            .block(Block::default().borders(Borders::NONE))
            .select(self.active)
            .highlight_style(active_style)
            .divider(Span::raw(" | "));

        frame.render_widget(tabs_widget, area);
    }
}
