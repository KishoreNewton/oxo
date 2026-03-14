//! Live diff mode — split-screen comparing two queries side by side.
//!
//! The [`DiffView`] renders a horizontally-split 50/50 layout. Each pane
//! shows log entries for its query. Entries are aligned by timestamp and
//! color-coded: entries only on the left are red, entries only on the right
//! are green.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use chrono::{DateTime, Utc};
use oxo_core::LogEntry;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Which pane is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffPane {
    Left,
    Right,
}

/// The current interaction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffMode {
    /// Browsing entries (normal mode).
    Viewing,
    /// Editing the left query.
    EditingLeft,
    /// Editing the right query.
    EditingRight,
}

/// A row in the aligned diff output.
#[derive(Debug)]
enum DiffRow {
    /// Entry present on both sides.
    Both(LogEntry, LogEntry),
    /// Entry only on the left.
    LeftOnly(LogEntry),
    /// Entry only on the right.
    RightOnly(LogEntry),
}

// ---------------------------------------------------------------------------
// DiffView
// ---------------------------------------------------------------------------

/// Split-screen component for comparing two query results side by side.
pub struct DiffView {
    visible: bool,
    theme: Theme,
    left_query: String,
    right_query: String,
    left_entries: Vec<LogEntry>,
    right_entries: Vec<LogEntry>,
    active_pane: DiffPane,
    scroll: usize,
    mode: DiffMode,
    query_input: Input,
}

impl DiffView {
    /// Create a new diff view.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            theme,
            left_query: String::new(),
            right_query: String::new(),
            left_entries: Vec::new(),
            right_entries: Vec::new(),
            active_pane: DiffPane::Left,
            scroll: 0,
            mode: DiffMode::Viewing,
            query_input: Input::default(),
        }
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.mode = DiffMode::Viewing;
        }
    }

    /// Whether the diff view is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Replace the left-hand entries.
    pub fn set_left_entries(&mut self, entries: Vec<LogEntry>) {
        self.left_entries = entries;
    }

    /// Replace the right-hand entries.
    pub fn set_right_entries(&mut self, entries: Vec<LogEntry>) {
        self.right_entries = entries;
    }

    /// Current left query string.
    pub fn left_query(&self) -> &str {
        &self.left_query
    }

    /// Current right query string.
    pub fn right_query(&self) -> &str {
        &self.right_query
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Produce an aligned diff of left and right entries, sorted by timestamp.
    fn aligned_rows(&self) -> Vec<DiffRow> {
        let mut rows = Vec::new();

        let mut li = 0;
        let mut ri = 0;

        while li < self.left_entries.len() && ri < self.right_entries.len() {
            let lt = self.left_entries[li].timestamp;
            let rt = self.right_entries[ri].timestamp;

            if lt == rt {
                rows.push(DiffRow::Both(
                    self.left_entries[li].clone(),
                    self.right_entries[ri].clone(),
                ));
                li += 1;
                ri += 1;
            } else if lt < rt {
                rows.push(DiffRow::LeftOnly(self.left_entries[li].clone()));
                li += 1;
            } else {
                rows.push(DiffRow::RightOnly(self.right_entries[ri].clone()));
                ri += 1;
            }
        }

        while li < self.left_entries.len() {
            rows.push(DiffRow::LeftOnly(self.left_entries[li].clone()));
            li += 1;
        }
        while ri < self.right_entries.len() {
            rows.push(DiffRow::RightOnly(self.right_entries[ri].clone()));
            ri += 1;
        }

        rows
    }

    /// Format a timestamp for display (HH:MM:SS.mmm).
    fn format_ts(ts: &DateTime<Utc>) -> String {
        ts.format("%H:%M:%S%.3f").to_string()
    }

    /// Render a single pane (left or right) into the given area.
    fn render_pane(
        &self,
        frame: &mut Frame,
        area: Rect,
        pane: DiffPane,
        rows: &[DiffRow],
        editing: bool,
    ) {
        let query = match pane {
            DiffPane::Left => &self.left_query,
            DiffPane::Right => &self.right_query,
        };

        let is_active = self.active_pane == pane;
        let border_style = if is_active {
            self.theme.border_focused()
        } else {
            self.theme.border_unfocused()
        };

        let title = match pane {
            DiffPane::Left => {
                if editing {
                    format!(" Left [editing]: {} ", self.query_input.value())
                } else {
                    format!(" Left: {} ", query)
                }
            }
            DiffPane::Right => {
                if editing {
                    format!(" Right [editing]: {} ", self.query_input.value())
                } else {
                    format!(" Right: {} ", query)
                }
            }
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let max_lines = inner.height as usize;
        let visible_rows = rows
            .iter()
            .skip(self.scroll)
            .take(max_lines);

        let left_only_style = Style::default().fg(Color::Red);
        let right_only_style = Style::default().fg(Color::Green);
        let both_style = Style::default().fg(self.theme.fg);
        let dim = Style::default().fg(self.theme.fg_dim);

        let lines: Vec<Line> = visible_rows
            .map(|row| {
                match (pane, row) {
                    (DiffPane::Left, DiffRow::Both(entry, _)) => {
                        let ts = Self::format_ts(&entry.timestamp);
                        Line::from(vec![
                            Span::styled(format!("{ts} "), dim),
                            Span::styled(&entry.line, both_style),
                        ])
                    }
                    (DiffPane::Right, DiffRow::Both(_, entry)) => {
                        let ts = Self::format_ts(&entry.timestamp);
                        Line::from(vec![
                            Span::styled(format!("{ts} "), dim),
                            Span::styled(&entry.line, both_style),
                        ])
                    }
                    (DiffPane::Left, DiffRow::LeftOnly(entry)) => {
                        let ts = Self::format_ts(&entry.timestamp);
                        Line::from(vec![
                            Span::styled(format!("{ts} "), dim),
                            Span::styled(&entry.line, left_only_style),
                        ])
                    }
                    (DiffPane::Right, DiffRow::LeftOnly(_)) => {
                        // Empty row on the right side for alignment.
                        Line::from(Span::raw(""))
                    }
                    (DiffPane::Right, DiffRow::RightOnly(entry)) => {
                        let ts = Self::format_ts(&entry.timestamp);
                        Line::from(vec![
                            Span::styled(format!("{ts} "), dim),
                            Span::styled(&entry.line, right_only_style),
                        ])
                    }
                    (DiffPane::Left, DiffRow::RightOnly(_)) => {
                        // Empty row on the left side for alignment.
                        Line::from(Span::raw(""))
                    }
                }
            })
            .collect();

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }

    /// Total number of aligned rows.
    fn row_count(&self) -> usize {
        // Quick count without allocating full rows.
        let mut count = 0;
        let mut li = 0;
        let mut ri = 0;
        while li < self.left_entries.len() && ri < self.right_entries.len() {
            let lt = self.left_entries[li].timestamp;
            let rt = self.right_entries[ri].timestamp;
            if lt == rt {
                li += 1;
                ri += 1;
            } else if lt < rt {
                li += 1;
            } else {
                ri += 1;
            }
            count += 1;
        }
        count += self.left_entries.len() - li;
        count += self.right_entries.len() - ri;
        count
    }
}

impl Component for DiffView {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }

        match self.mode {
            DiffMode::Viewing => match key.code {
                KeyCode::Esc => {
                    self.visible = false;
                    Some(Action::Noop)
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    let max = self.row_count();
                    if self.scroll < max.saturating_sub(1) {
                        self.scroll += 1;
                    }
                    Some(Action::Noop)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.scroll = self.scroll.saturating_sub(1);
                    Some(Action::Noop)
                }
                KeyCode::Tab => {
                    self.active_pane = match self.active_pane {
                        DiffPane::Left => DiffPane::Right,
                        DiffPane::Right => DiffPane::Left,
                    };
                    Some(Action::Noop)
                }
                KeyCode::Char('l') => {
                    self.mode = DiffMode::EditingLeft;
                    self.query_input = Input::new(self.left_query.clone());
                    Some(Action::Noop)
                }
                KeyCode::Char('r') => {
                    self.mode = DiffMode::EditingRight;
                    self.query_input = Input::new(self.right_query.clone());
                    Some(Action::Noop)
                }
                _ => Some(Action::Noop),
            },
            DiffMode::EditingLeft => match key.code {
                KeyCode::Esc => {
                    self.mode = DiffMode::Viewing;
                    Some(Action::Noop)
                }
                KeyCode::Enter => {
                    let query = self.query_input.value().to_string();
                    self.left_query = query.clone();
                    self.mode = DiffMode::Viewing;
                    Some(Action::DiffQueryLeft(query))
                }
                _ => {
                    self.query_input.handle_event(
                        &crossterm::event::Event::Key(key),
                    );
                    Some(Action::Noop)
                }
            },
            DiffMode::EditingRight => match key.code {
                KeyCode::Esc => {
                    self.mode = DiffMode::Viewing;
                    Some(Action::Noop)
                }
                KeyCode::Enter => {
                    let query = self.query_input.value().to_string();
                    self.right_query = query.clone();
                    self.mode = DiffMode::Viewing;
                    Some(Action::DiffQueryRight(query))
                }
                _ => {
                    self.query_input.handle_event(
                        &crossterm::event::Event::Key(key),
                    );
                    Some(Action::Noop)
                }
            },
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        if !self.visible {
            return;
        }

        let rows = self.aligned_rows();

        // Split horizontally 50/50.
        let chunks = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

        let editing_left = self.mode == DiffMode::EditingLeft;
        let editing_right = self.mode == DiffMode::EditingRight;

        self.render_pane(frame, chunks[0], DiffPane::Left, &rows, editing_left);
        self.render_pane(frame, chunks[1], DiffPane::Right, &rows, editing_right);

        // Render status hint at the bottom of the area.
        if area.height > 2 {
            let hint_area = Rect {
                x: area.x,
                y: area.y + area.height - 1,
                width: area.width,
                height: 1,
            };
            let hint = match self.mode {
                DiffMode::Viewing => {
                    " j/k: scroll | Tab: switch pane | l: edit left | r: edit right | Esc: close "
                }
                DiffMode::EditingLeft | DiffMode::EditingRight => {
                    " Enter: submit query | Esc: cancel "
                }
            };
            let hint_style = Style::default()
                .fg(self.theme.status_fg)
                .bg(self.theme.status_bg)
                .add_modifier(Modifier::ITALIC);
            let hint_line = Line::from(Span::styled(hint, hint_style));
            let hint_widget = Paragraph::new(hint_line);
            frame.render_widget(hint_widget, hint_area);
        }
    }
}
