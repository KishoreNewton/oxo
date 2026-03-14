//! Saved views overlay component.
//!
//! Displays a centered popup listing all saved views and allows the user to
//! load or delete them. Similar in behaviour to the source picker overlay.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;
use crate::views::SavedView;

/// Overlay panel for browsing and loading saved views.
pub struct SavedViewsPanel {
    /// Whether the popup is currently shown.
    visible: bool,
    /// The list of views to display.
    views: Vec<SavedView>,
    /// Cursor position in the list.
    cursor: usize,
    /// Color theme.
    theme: Theme,
}

impl SavedViewsPanel {
    /// Create a new saved-views panel.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            views: Vec::new(),
            cursor: 0,
            theme,
        }
    }

    /// Toggle visibility, resetting the cursor to the top.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.cursor = 0;
        }
    }

    /// Replace the displayed views list.
    pub fn set_views(&mut self, views: Vec<SavedView>) {
        self.views = views;
        self.cursor = 0;
    }

    /// Whether there are any views to show.
    pub fn has_views(&self) -> bool {
        !self.views.is_empty()
    }

    /// Whether the popup is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

impl Component for SavedViewsPanel {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.views.is_empty() && self.cursor + 1 < self.views.len() {
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
                if let Some(view) = self.views.get(self.cursor) {
                    let name = view.name.clone();
                    self.visible = false;
                    Some(Action::Notify(format!("Loading view: {}", name)))
                } else {
                    self.visible = false;
                    Some(Action::Noop)
                }
            }
            KeyCode::Char('d') => {
                if let Some(view) = self.views.get(self.cursor) {
                    let name = view.name.clone();
                    self.views.remove(self.cursor);
                    // Adjust cursor if we removed the last item.
                    if self.cursor >= self.views.len() && self.cursor > 0 {
                        self.cursor -= 1;
                    }
                    Some(Action::Notify(format!("Deleted view: {}", name)))
                } else {
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
        if !self.visible {
            return;
        }

        // Show a message if there are no views.
        let content_lines = if self.views.is_empty() {
            vec![
                Line::from(""),
                Line::from(Span::styled("  No saved views yet.", self.theme.dimmed())),
                Line::from(""),
                Line::from(Span::styled("  [Esc] close", self.theme.dimmed())),
            ]
        } else {
            let accent_bold = Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD);
            let normal = Style::default().fg(self.theme.fg);
            let dim = self.theme.dimmed();

            let mut lines: Vec<Line> = Vec::with_capacity(self.views.len() * 3 + 4);
            lines.push(Line::from(""));

            for (i, view) in self.views.iter().enumerate() {
                let is_cursor = i == self.cursor;

                // First line: marker + name.
                let marker = if is_cursor { " > " } else { "   " };
                let name_style = if is_cursor { accent_bold } else { normal };

                lines.push(Line::from(vec![
                    Span::styled(marker, name_style),
                    Span::styled(&view.name, name_style),
                ]));

                // Second line: query preview (truncated).
                let query_preview = if view.query.len() > 35 {
                    format!("     {} ...", &view.query[..35])
                } else {
                    format!("     {}", &view.query)
                };
                lines.push(Line::from(Span::styled(query_preview, dim)));

                // Third line: time range + source.
                let mut meta = format!("     {}m", view.time_range_minutes);
                if let Some(ref src) = view.source {
                    meta.push_str(&format!(" | {}", src));
                }
                lines.push(Line::from(Span::styled(meta, dim)));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  [Enter] load  [d] delete  [Esc] close",
                dim,
            )));
            lines
        };

        let popup_height = (content_lines.len() as u16 + 2).min(area.height.saturating_sub(4));
        let popup_width = 50u16.min(area.width.saturating_sub(4));

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Saved Views ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let paragraph = Paragraph::new(content_lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }
}
