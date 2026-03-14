//! Time range picker overlay component.
//!
//! Displays a centered popup that lets the user choose a preset relative time
//! range for log queries (e.g. "Last 1 hour").  Navigation is vim-style (j/k)
//! and the currently active selection is visually distinguished from the
//! cursor position.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// A preset relative time range option.
#[derive(Debug, Clone, Copy)]
pub struct TimePreset {
    pub label: &'static str,
    /// Duration expressed in minutes.
    pub minutes: u64,
}

const PRESETS: &[TimePreset] = &[
    TimePreset {
        label: "Last 5 minutes",
        minutes: 5,
    },
    TimePreset {
        label: "Last 15 minutes",
        minutes: 15,
    },
    TimePreset {
        label: "Last 30 minutes",
        minutes: 30,
    },
    TimePreset {
        label: "Last 1 hour",
        minutes: 60,
    },
    TimePreset {
        label: "Last 3 hours",
        minutes: 180,
    },
    TimePreset {
        label: "Last 6 hours",
        minutes: 360,
    },
    TimePreset {
        label: "Last 12 hours",
        minutes: 720,
    },
    TimePreset {
        label: "Last 24 hours",
        minutes: 1440,
    },
    TimePreset {
        label: "Last 2 days",
        minutes: 2880,
    },
    TimePreset {
        label: "Last 7 days",
        minutes: 10080,
    },
];

/// Overlay component that presents a list of preset time ranges.
pub struct TimePicker {
    /// Whether the popup is currently shown.
    visible: bool,
    /// The row the cursor is currently on (not yet confirmed).
    cursor: usize,
    /// The index of the preset that is currently active / confirmed.
    selected: usize,
    theme: Theme,
}

impl TimePicker {
    /// Create a new time picker.  Defaults to "Last 1 hour" (index 3).
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            cursor: 3,
            selected: 3,
            theme,
        }
    }

    /// Show/hide the picker, resetting the cursor to the active selection.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            // Always open with cursor on the currently active preset.
            self.cursor = self.selected;
        }
    }

    /// Whether the picker popup is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Minutes for the currently confirmed time range.
    pub fn selected_minutes(&self) -> u64 {
        PRESETS[self.selected].minutes
    }

    /// Human-readable label for the currently confirmed time range.
    pub fn selected_label(&self) -> &'static str {
        PRESETS[self.selected].label
    }

    // ── private helpers ────────────────────────────────────────────────────

    fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn cursor_down(&mut self) {
        if self.cursor + 1 < PRESETS.len() {
            self.cursor += 1;
        }
    }

    fn confirm(&mut self) -> Action {
        self.selected = self.cursor;
        self.visible = false;
        Action::SetTimeRange(PRESETS[self.selected].minutes)
    }
}

impl Component for TimePicker {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.cursor_down();
                Some(Action::Noop)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cursor_up();
                Some(Action::Noop)
            }
            KeyCode::Enter => Some(self.confirm()),
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

        // Size the popup to fit all presets plus chrome (border + padding + footer).
        let popup_height = (PRESETS.len() as u16 + 4).min(area.height.saturating_sub(4));
        // Wide enough for the longest label plus markers and padding.
        let popup_width = 28u16.min(area.width.saturating_sub(4));

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Time Range ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let accent_bold = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let normal = Style::default().fg(self.theme.fg);
        let dim = self.theme.dimmed();

        let mut lines: Vec<Line> = Vec::with_capacity(PRESETS.len() + 3);

        // One blank line at the top for visual breathing room.
        lines.push(Line::from(""));

        for (i, preset) in PRESETS.iter().enumerate() {
            let is_cursor = i == self.cursor;
            let is_selected = i == self.selected;

            let line = if is_cursor && is_selected {
                // Cursor AND active: accent, bold, markers on both sides.
                Line::from(vec![
                    Span::styled(" ► ", accent_bold),
                    Span::styled(preset.label, accent_bold),
                    Span::styled(" ◄", accent_bold),
                ])
            } else if is_cursor {
                // Cursor only: accent, bold, left marker.
                Line::from(vec![
                    Span::styled(" ► ", accent_bold),
                    Span::styled(preset.label, accent_bold),
                ])
            } else if is_selected {
                // Active but cursor is elsewhere: dim accent, right marker.
                Line::from(vec![
                    Span::styled("   ", normal),
                    Span::styled(preset.label, Style::default().fg(self.theme.accent)),
                    Span::styled(" ◄", dim),
                ])
            } else {
                // Plain row.
                Line::from(vec![
                    Span::styled("   ", normal),
                    Span::styled(preset.label, normal),
                ])
            };

            lines.push(line);
        }

        // Footer hint.
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  [Enter] select  [Esc] cancel",
            dim,
        )));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }
}
