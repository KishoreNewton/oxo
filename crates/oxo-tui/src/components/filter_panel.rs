//! Label filter panel component.
//!
//! Displays a sidebar with available labels and their values, allowing
//! users to quickly filter log streams by toggling label matchers.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// A label with its known values.
#[derive(Debug, Clone)]
pub struct LabelGroup {
    /// The label name (e.g. "namespace", "level").
    pub name: String,
    /// Known values for this label.
    pub values: Vec<String>,
    /// Whether this group is expanded in the UI.
    pub expanded: bool,
}

/// An active filter (label = value).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveFilter {
    pub label: String,
    pub value: String,
}

/// Filter panel component state.
pub struct FilterPanel {
    /// Available label groups.
    labels: Vec<LabelGroup>,
    /// Currently active filters.
    active_filters: Vec<ActiveFilter>,
    /// Whether the panel is visible.
    visible: bool,
    /// List selection state.
    list_state: ListState,
    /// Flat list of selectable items (for navigation).
    flat_items: Vec<FlatItem>,
    /// Color theme.
    theme: Theme,
}

/// A flattened item in the filter list (for navigation).
#[derive(Debug, Clone)]
enum FlatItem {
    /// A label group header.
    Label(usize),
    /// A value under a label group.
    Value(usize, usize),
}

impl FilterPanel {
    /// Create a new filter panel.
    pub fn new(theme: Theme) -> Self {
        Self {
            labels: Vec::new(),
            active_filters: Vec::new(),
            visible: false,
            list_state: ListState::default(),
            flat_items: Vec::new(),
            theme,
        }
    }

    /// Whether the panel is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Toggle panel visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Get the list of active filters.
    pub fn active_filters(&self) -> &[ActiveFilter] {
        &self.active_filters
    }

    /// Update the available labels (called after backend.labels() returns).
    pub fn set_labels(&mut self, names: Vec<String>) {
        self.labels = names
            .into_iter()
            .map(|name| LabelGroup {
                name,
                values: Vec::new(),
                expanded: false,
            })
            .collect();
        self.rebuild_flat_items();
    }

    /// Update the values for a specific label.
    pub fn set_label_values(&mut self, label: &str, values: Vec<String>) {
        if let Some(group) = self.labels.iter_mut().find(|g| g.name == label) {
            group.values = values;
            self.rebuild_flat_items();
        }
    }

    /// Rebuild the flat item list after labels/values change.
    fn rebuild_flat_items(&mut self) {
        self.flat_items.clear();
        for (li, group) in self.labels.iter().enumerate() {
            self.flat_items.push(FlatItem::Label(li));
            if group.expanded {
                for vi in 0..group.values.len() {
                    self.flat_items.push(FlatItem::Value(li, vi));
                }
            }
        }
    }

    /// Check if a given label=value filter is active.
    fn is_filter_active(&self, label: &str, value: &str) -> bool {
        self.active_filters
            .iter()
            .any(|f| f.label == label && f.value == value)
    }

    /// Toggle a filter on or off.
    fn toggle_filter(&mut self, label: String, value: String) -> Option<Action> {
        if let Some(pos) = self
            .active_filters
            .iter()
            .position(|f| f.label == label && f.value == value)
        {
            self.active_filters.remove(pos);
        } else {
            self.active_filters.push(ActiveFilter {
                label: label.clone(),
                value: value.clone(),
            });
        }
        Some(Action::SetFilter { label, value })
    }
}

impl Component for FilterPanel {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let len = self.flat_items.len();
                if len > 0 {
                    let i = self.list_state.selected().map_or(0, |i| (i + 1) % len);
                    self.list_state.select(Some(i));
                }
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let len = self.flat_items.len();
                if len > 0 {
                    let i = self
                        .list_state
                        .selected()
                        .map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
                    self.list_state.select(Some(i));
                }
                None
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(idx) = self.list_state.selected() {
                    if let Some(item) = self.flat_items.get(idx).cloned() {
                        match item {
                            FlatItem::Label(li) => {
                                // Toggle expand/collapse.
                                if let Some(group) = self.labels.get_mut(li) {
                                    group.expanded = !group.expanded;
                                    self.rebuild_flat_items();
                                }
                                None
                            }
                            FlatItem::Value(li, vi) => {
                                let label = self.labels[li].name.clone();
                                let value = self.labels[li].values[vi].clone();
                                self.toggle_filter(label, value)
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        if !self.visible {
            return;
        }

        let border_style = if focused {
            self.theme.border_focused()
        } else {
            self.theme.border_unfocused()
        };

        let block = Block::default()
            .title(" Filters ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let items: Vec<ListItem> = self
            .flat_items
            .iter()
            .map(|item| match item {
                FlatItem::Label(li) => {
                    let group = &self.labels[*li];
                    let arrow = if group.expanded { "▾" } else { "▸" };
                    ListItem::new(Line::from(vec![Span::styled(
                        format!("{arrow} {}", group.name),
                        Style::default().add_modifier(Modifier::BOLD),
                    )]))
                }
                FlatItem::Value(li, vi) => {
                    let group = &self.labels[*li];
                    let value = &group.values[*vi];
                    let marker = if self.is_filter_active(&group.name, value) {
                        "●"
                    } else {
                        "○"
                    };
                    ListItem::new(Line::from(vec![Span::raw(format!("  {marker} {value}"))]))
                }
            })
            .collect();

        let list = List::new(items).block(block).highlight_style(
            Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD),
        );

        frame.render_stateful_widget(list, area, &mut self.list_state.clone());
    }
}
