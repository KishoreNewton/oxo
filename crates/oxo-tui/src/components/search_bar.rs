//! Search input bar component.
//!
//! A minimal text input that appears at the bottom of the log viewer
//! when the user presses `/` in search mode. Submits a search term
//! on Enter and supports incremental search.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Search bar component.
pub struct SearchBar {
    /// The text input.
    input: Input,
    /// Whether search mode is active.
    active: bool,
    /// Color theme.
    theme: Theme,
}

impl SearchBar {
    /// Create a new search bar.
    pub fn new(theme: Theme) -> Self {
        Self {
            input: Input::default(),
            active: false,
            theme,
        }
    }

    /// Whether the search bar is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Activate the search bar.
    pub fn activate(&mut self) {
        self.active = true;
        self.input = Input::default();
    }

    /// Deactivate the search bar.
    pub fn deactivate(&mut self) {
        self.active = false;
    }
}

impl Component for SearchBar {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.active {
            return None;
        }

        match key.code {
            KeyCode::Enter => {
                let term = self.input.value().to_string();
                self.active = false;
                if term.is_empty() {
                    Some(Action::SearchClear)
                } else {
                    Some(Action::SearchSubmit(term))
                }
            }
            KeyCode::Esc => {
                self.active = false;
                Some(Action::ExitSearchMode)
            }
            _ => {
                self.input.handle_event(&crossterm::event::Event::Key(key));
                // Incremental search: update highlights as user types.
                let term = self.input.value().to_string();
                if term.is_empty() {
                    Some(Action::SearchClear)
                } else {
                    Some(Action::SearchSubmit(term))
                }
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        if !self.active {
            return;
        }

        let line = Line::from(vec![
            Span::styled(
                "/",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(self.input.value()),
        ]);

        let paragraph = Paragraph::new(line).style(self.theme.status_bar());
        frame.render_widget(paragraph, area);

        // Show cursor.
        let cursor_x = area.x + 1 + self.input.visual_cursor() as u16;
        let cursor_y = area.y;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
