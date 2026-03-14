//! Source picker overlay component.
//!
//! Displays a centered popup listing all configured log sources and allows the
//! user to switch between them at runtime.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Metadata for a source shown in the picker.
#[derive(Debug, Clone)]
pub struct SourceEntry {
    /// Display name (the key from `[sources.<name>]` in config).
    pub name: String,
    /// Backend type (e.g. "loki", "demo").
    pub backend: String,
    /// Connection URL for display purposes.
    pub url: String,
}

/// Source picker overlay component.
pub struct SourcePicker {
    /// Whether the popup is currently shown.
    visible: bool,
    /// Available sources.
    sources: Vec<SourceEntry>,
    /// Cursor position in the list.
    cursor: usize,
    /// Index of the currently active source.
    active: usize,
    theme: Theme,
}

impl SourcePicker {
    /// Create a new source picker.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            sources: Vec::new(),
            cursor: 0,
            active: 0,
            theme,
        }
    }

    /// Set the available sources. The first entry is treated as the initially
    /// active source.
    pub fn set_sources(&mut self, sources: Vec<SourceEntry>) {
        self.sources = sources;
        self.cursor = 0;
        self.active = 0;
    }

    /// Show or hide the picker, resetting the cursor to the active source.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.cursor = self.active;
        }
    }

    /// Whether the popup is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Mark a source as the active one by name.
    pub fn set_active_by_name(&mut self, name: &str) {
        if let Some(pos) = self.sources.iter().position(|s| s.name == name) {
            self.active = pos;
        }
    }

    /// Whether there are any sources to show.
    pub fn has_sources(&self) -> bool {
        !self.sources.is_empty()
    }
}

impl Component for SourcePicker {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.sources.is_empty() && self.cursor + 1 < self.sources.len() {
                    self.cursor += 1;
                }
                Some(Action::Noop)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                if let Some(source) = self.sources.get(self.cursor) {
                    let name = source.name.clone();
                    self.active = self.cursor;
                    self.visible = false;
                    Some(Action::SwitchSource(name))
                } else {
                    self.visible = false;
                    Some(Action::Noop)
                }
            }
            KeyCode::Esc => {
                self.visible = false;
                Some(Action::Noop)
            }
            _ => Some(Action::Noop), // Consume all keys while visible.
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        if !self.visible || self.sources.is_empty() {
            return;
        }

        let popup_height = (self.sources.len() as u16 + 5).min(area.height.saturating_sub(4));
        let popup_width = 45u16.min(area.width.saturating_sub(4));

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Switch Source ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let accent_bold = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let normal = Style::default().fg(self.theme.fg);
        let dim = self.theme.dimmed();

        let mut lines: Vec<Line> = Vec::with_capacity(self.sources.len() + 4);
        lines.push(Line::from(""));

        for (i, source) in self.sources.iter().enumerate() {
            let is_cursor = i == self.cursor;
            let is_active = i == self.active;

            let detail = format!("{} ({})", source.name, source.backend);

            let line = if is_cursor && is_active {
                Line::from(vec![
                    Span::styled(" ► ", accent_bold),
                    Span::styled(detail, accent_bold),
                    Span::styled(" ◄", accent_bold),
                ])
            } else if is_cursor {
                Line::from(vec![
                    Span::styled(" ► ", accent_bold),
                    Span::styled(detail, accent_bold),
                ])
            } else if is_active {
                Line::from(vec![
                    Span::styled("   ", normal),
                    Span::styled(detail, Style::default().fg(self.theme.accent)),
                    Span::styled(" ◄", dim),
                ])
            } else {
                Line::from(vec![
                    Span::styled("   ", normal),
                    Span::styled(detail, normal),
                ])
            };
            lines.push(line);
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  [Enter] switch  [Esc] cancel",
            dim,
        )));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }
}
