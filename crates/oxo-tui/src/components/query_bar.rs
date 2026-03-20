//! Query input bar component.
//!
//! Displays a text input at the top of the screen where users type
//! LogQL (or other backend-native) queries. Supports cursor movement,
//! history navigation, submission on Enter, and autocomplete for label
//! names and values.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::action::Action;
use crate::components::Component;
use crate::components::autocomplete::AutocompletePopup;
use crate::theme::Theme;

/// Maximum number of queries to keep in history.
const MAX_HISTORY: usize = 50;

/// What kind of completion context the cursor is in.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CompletionContext {
    /// Cursor is in a position where a label name is expected
    /// (after `{`, or after `, ` inside braces).
    LabelName {
        /// The partial text typed so far for filtering.
        prefix: String,
        /// Byte offset in the input where the partial token starts.
        token_start: usize,
    },
    /// Cursor is after `label_name="` — expecting a label value.
    LabelValue {
        /// The label name we are completing values for.
        label: String,
        /// The partial value typed so far.
        prefix: String,
        /// Byte offset in the input where the partial value starts.
        token_start: usize,
    },
    /// No completion context detected.
    None,
}

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
    /// Autocomplete popup.
    autocomplete: AutocompletePopup,
    /// Cached label names from the backend.
    cached_labels: Vec<String>,
    /// Cached label values keyed by label name.
    cached_label_values: HashMap<String, Vec<String>>,
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
            autocomplete: AutocompletePopup::new(theme.clone()),
            cached_labels: Vec::new(),
            cached_label_values: HashMap::new(),
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
        self.autocomplete.hide();
    }

    /// Get the currently submitted query.
    pub fn current_query(&self) -> &str {
        &self.current_query
    }

    /// Set the list of available label names (called when backend responds).
    pub fn set_available_labels(&mut self, labels: Vec<String>) {
        self.cached_labels = labels;
    }

    /// Set cached values for a specific label (called when backend responds).
    pub fn set_label_values(&mut self, label: &str, values: Vec<String>) {
        self.cached_label_values.insert(label.to_string(), values);
    }

    /// Submit the current input as a query.
    fn submit(&mut self) -> Option<Action> {
        self.autocomplete.hide();
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

    /// Accept the currently selected autocomplete suggestion, inserting it
    /// into the input at the correct position.
    fn accept_suggestion(&mut self) {
        let suggestion = match self.autocomplete.selected_item() {
            Some(s) => s.to_string(),
            None => return,
        };

        let ctx = self.analyze_context();
        let value = self.input.value().to_string();

        let new_value = match ctx {
            CompletionContext::LabelName { token_start, .. } => {
                let cursor_byte = char_to_byte_offset(&value, self.input.cursor());
                format!(
                    "{}{}{}",
                    &value[..token_start],
                    suggestion,
                    &value[cursor_byte..],
                )
            }
            CompletionContext::LabelValue { token_start, .. } => {
                let cursor_byte = char_to_byte_offset(&value, self.input.cursor());
                format!(
                    "{}{}{}",
                    &value[..token_start],
                    suggestion,
                    &value[cursor_byte..],
                )
            }
            CompletionContext::None => return,
        };

        // Compute where the cursor should end up (right after the inserted text).
        let insert_start_chars = match ctx {
            CompletionContext::LabelName { token_start, .. }
            | CompletionContext::LabelValue { token_start, .. } => {
                byte_to_char_offset(&value, token_start)
            }
            CompletionContext::None => 0,
        };
        let new_cursor = insert_start_chars + suggestion.chars().count();

        self.input = Input::new(new_value).with_cursor(new_cursor);
        self.autocomplete.hide();
    }

    /// Analyze the cursor context in the query text to determine what kind
    /// of autocomplete suggestions to show.
    fn analyze_context(&self) -> CompletionContext {
        let value = self.input.value();
        let cursor = self.input.cursor();

        // Work with the byte slice up to the cursor position.
        let cursor_byte = char_to_byte_offset(value, cursor);
        let before_cursor = &value[..cursor_byte];

        // Find the innermost unclosed `{` before the cursor.
        let brace_open = before_cursor.rfind('{');
        let brace_close = before_cursor.rfind('}');

        let inside_braces = match (brace_open, brace_close) {
            (Some(open), Some(close)) => open > close,
            (Some(_), None) => true,
            _ => false,
        };

        if !inside_braces {
            return CompletionContext::None;
        }

        let brace_pos = brace_open.unwrap();
        let inside = &before_cursor[brace_pos + 1..];

        // Check if we're after `="` (label value context).
        // Find the last `="` in the inside-braces portion.
        if let Some(eq_quote_pos) = inside.rfind("=\"") {
            // Make sure there's no closing `"` after the `="` that would mean
            // the value is already complete.
            let after_eq_quote = &inside[eq_quote_pos + 2..];
            if !after_eq_quote.contains('"') {
                // We're inside an open value string. Extract the label name
                // and the partial value.
                let before_eq = &inside[..eq_quote_pos];
                // The label name is the last comma-separated token before `=`.
                let label = before_eq
                    .rsplit(',')
                    .next()
                    .unwrap_or(before_eq)
                    .trim()
                    .to_string();
                let prefix = after_eq_quote.to_string();
                let token_start = brace_pos + 1 + eq_quote_pos + 2;

                return CompletionContext::LabelValue {
                    label,
                    prefix,
                    token_start,
                };
            }
        }

        // Check if we're after `=~"` (regex match — also offer value suggestions).
        if let Some(eq_tilde_pos) = inside.rfind("=~\"") {
            let after = &inside[eq_tilde_pos + 3..];
            if !after.contains('"') {
                let before_eq = &inside[..eq_tilde_pos];
                let label = before_eq
                    .rsplit(',')
                    .next()
                    .unwrap_or(before_eq)
                    .trim()
                    .to_string();
                let prefix = after.to_string();
                let token_start = brace_pos + 1 + eq_tilde_pos + 3;

                return CompletionContext::LabelValue {
                    label,
                    prefix,
                    token_start,
                };
            }
        }

        // Otherwise we're in label-name position.
        // The partial label name is whatever comes after the last `,` (or after `{`).
        let partial = inside.rsplit(',').next().unwrap_or(inside).trim_start();

        // If partial contains `=` or `!` we're past the label name — no completion.
        if partial.contains('=') || partial.contains('!') {
            return CompletionContext::None;
        }

        let prefix = partial.to_string();
        let token_start = cursor_byte - prefix.len();

        CompletionContext::LabelName {
            prefix,
            token_start,
        }
    }

    /// Re-evaluate the autocomplete context after the input changes.
    /// Returns an optional action to request data from the backend.
    fn update_autocomplete(&mut self) -> Option<Action> {
        let ctx = self.analyze_context();

        match ctx {
            CompletionContext::LabelName { ref prefix, .. } => {
                if self.cached_labels.is_empty() {
                    // Request labels from the backend.
                    self.autocomplete.hide();
                    return Some(Action::AutocompleteLabels);
                }
                self.autocomplete.set_items(self.cached_labels.clone());
                self.autocomplete.set_filter(prefix);
                self.autocomplete.show();
                None
            }
            CompletionContext::LabelValue {
                ref label,
                ref prefix,
                ..
            } => {
                if let Some(values) = self.cached_label_values.get(label) {
                    self.autocomplete.set_items(values.clone());
                    self.autocomplete.set_filter(prefix);
                    self.autocomplete.show();
                    None
                } else {
                    // Request values for this label.
                    self.autocomplete.hide();
                    Some(Action::AutocompleteLabelValues(label.clone()))
                }
            }
            CompletionContext::None => {
                self.autocomplete.hide();
                None
            }
        }
    }
}

impl Component for QueryBar {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.active {
            return None;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // When autocomplete is visible, intercept navigation keys.
        if self.autocomplete.is_visible() {
            match key.code {
                KeyCode::Tab => {
                    self.accept_suggestion();
                    return self.update_autocomplete();
                }
                KeyCode::Down | KeyCode::Char('n') if ctrl || key.code == KeyCode::Down => {
                    self.autocomplete.next();
                    return None;
                }
                KeyCode::Up | KeyCode::Char('p') if ctrl || key.code == KeyCode::Up => {
                    self.autocomplete.prev();
                    return None;
                }
                KeyCode::Esc => {
                    self.autocomplete.hide();
                    return None;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Enter => self.submit(),
            KeyCode::Esc => {
                self.deactivate();
                Some(Action::ExitQueryMode)
            }
            KeyCode::Up if !self.autocomplete.is_visible() => {
                self.history_prev();
                None
            }
            KeyCode::Down if !self.autocomplete.is_visible() => {
                self.history_next();
                None
            }
            _ => {
                // Delegate to tui-input for cursor movement, deletion, etc.
                let before = self.input.value().to_string();
                self.input.handle_event(&crossterm::event::Event::Key(key));
                let after = self.input.value();

                // If the text changed, re-evaluate autocomplete context.
                if before != after {
                    self.update_autocomplete()
                } else {
                    None
                }
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

        // Show autocomplete popup below the query bar.
        if self.active {
            self.autocomplete.render(frame, area);
        }
    }
}

/// Convert a char-index cursor position to a byte offset in the string.
fn char_to_byte_offset(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

/// Convert a byte offset to a char-index position in the string.
fn byte_to_char_offset(s: &str, byte_offset: usize) -> usize {
    s[..byte_offset].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn make_bar(text: &str) -> QueryBar {
        let mut bar = QueryBar::new(Theme::default(), Some(text.to_string()));
        bar.active = true;
        bar.cached_labels = vec![
            "app".to_string(),
            "env".to_string(),
            "host".to_string(),
            "level".to_string(),
        ];
        bar.cached_label_values.insert(
            "app".to_string(),
            vec!["frontend".to_string(), "backend".to_string()],
        );
        bar
    }

    #[test]
    fn context_label_name_after_brace() {
        let bar = make_bar("{");
        let ctx = bar.analyze_context();
        assert!(matches!(ctx, CompletionContext::LabelName { .. }));
        if let CompletionContext::LabelName { prefix, .. } = ctx {
            assert_eq!(prefix, "");
        }
    }

    #[test]
    fn context_label_name_partial() {
        let bar = make_bar("{ap");
        let ctx = bar.analyze_context();
        assert!(matches!(ctx, CompletionContext::LabelName { .. }));
        if let CompletionContext::LabelName { prefix, .. } = ctx {
            assert_eq!(prefix, "ap");
        }
    }

    #[test]
    fn context_label_name_after_comma() {
        let mut bar = make_bar("{app=\"frontend\", le");
        bar.input = Input::new("{app=\"frontend\", le".to_string());
        let ctx = bar.analyze_context();
        assert!(matches!(ctx, CompletionContext::LabelName { .. }));
        if let CompletionContext::LabelName { prefix, .. } = ctx {
            assert_eq!(prefix, "le");
        }
    }

    #[test]
    fn context_label_value() {
        let bar = make_bar("{app=\"front");
        let ctx = bar.analyze_context();
        assert!(matches!(ctx, CompletionContext::LabelValue { .. }));
        if let CompletionContext::LabelValue { label, prefix, .. } = ctx {
            assert_eq!(label, "app");
            assert_eq!(prefix, "front");
        }
    }

    #[test]
    fn context_none_outside_braces() {
        let bar = make_bar("rate(");
        let ctx = bar.analyze_context();
        assert!(matches!(ctx, CompletionContext::None));
    }

    #[test]
    fn accept_label_name_suggestion() {
        let mut bar = make_bar("{ap");
        bar.update_autocomplete();
        // "app" should be the first filtered match.
        assert!(bar.autocomplete.is_visible());
        bar.accept_suggestion();
        assert_eq!(bar.input.value(), "{app");
    }

    #[test]
    fn accept_label_value_suggestion() {
        let mut bar = make_bar("{app=\"front");
        bar.update_autocomplete();
        assert!(bar.autocomplete.is_visible());
        bar.accept_suggestion();
        assert_eq!(bar.input.value(), "{app=\"frontend");
    }
}
