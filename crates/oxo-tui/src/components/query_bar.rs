//! Query input bar component.
//!
//! Displays a text input at the top of the screen where users type
//! LogQL (or other backend-native) queries. Supports cursor movement,
//! history navigation, and submission on Enter.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Maximum number of queries to keep in history.
const MAX_HISTORY: usize = 50;

/// Query bar component state.
pub struct QueryBar {
    /// The text input widget.
    input: Input,
    /// Whether the query bar is currently active (accepting input).
    active: bool,
    /// Query history (most recent last).
    history: Vec<String>,
    /// Current position in history (when navigating with up/down).
    history_index: Option<usize>,
    /// The current query that has been submitted.
    current_query: String,
    /// Color theme.
    theme: Theme,
}

impl QueryBar {
    /// Create a new query bar with an optional initial query.
    pub fn new(theme: Theme, initial_query: Option<String>) -> Self {
        let query = initial_query.unwrap_or_default();
        Self {
            input: Input::new(query.clone()),
            active: false,
            history: Vec::new(),
            history_index: None,
            current_query: query,
            theme,
        }
    }

    /// Whether the query bar is currently active (in input mode).
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Activate the query bar for input.
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Deactivate the query bar.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.history_index = None;
    }

    /// Get the currently submitted query.
    pub fn current_query(&self) -> &str {
        &self.current_query
    }

    /// Submit the current input as a query.
    fn submit(&mut self) -> Option<Action> {
        let query = self.input.value().to_string();
        if query.is_empty() {
            return None;
        }

        // Add to history (avoid duplicates of the last entry).
        if self.history.last().map(|s| s.as_str()) != Some(&query) {
            self.history.push(query.clone());
            if self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
        }

        self.current_query = query.clone();
        self.history_index = None;
        self.active = false;

        Some(Action::SubmitQuery(query))
    }

    /// Navigate to the previous history entry.
    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_index {
            Some(i) => i.saturating_sub(1),
            None => self.history.len() - 1,
        };
        self.history_index = Some(idx);
        self.input = Input::new(self.history[idx].clone());
    }

    /// Navigate to the next history entry.
    fn history_next(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.history.len() {
                self.history_index = Some(idx + 1);
                self.input = Input::new(self.history[idx + 1].clone());
            } else {
                // Past the end — return to empty input.
                self.history_index = None;
                self.input = Input::new(self.current_query.clone());
            }
        }
    }
}

impl Component for QueryBar {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.active {
            return None;
        }

        match key.code {
            KeyCode::Enter => self.submit(),
            KeyCode::Esc => {
                self.deactivate();
                Some(Action::ExitQueryMode)
            }
            KeyCode::Up => {
                self.history_prev();
                None
            }
            KeyCode::Down => {
                self.history_next();
                None
            }
            _ => {
                // Delegate to tui-input for cursor movement, deletion, etc.
                self.input.handle_event(&crossterm::event::Event::Key(key));
                None
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let border_style = if focused || self.active {
            self.theme.border_focused()
        } else {
            self.theme.border_unfocused()
        };

        let prompt = if self.active { "Query> " } else { "Query: " };

        let display_value = if self.active {
            self.input.value()
        } else {
            &self.current_query
        };

        let line = Line::from(vec![
            Span::styled(prompt, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(display_value),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(line).block(block);
        frame.render_widget(paragraph, area);

        // Show cursor when active.
        if self.active {
            let cursor_x = area.x + 1 + prompt.len() as u16 + self.input.visual_cursor() as u16;
            let cursor_y = area.y + 1;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}
