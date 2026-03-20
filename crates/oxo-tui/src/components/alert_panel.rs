//! Alert history panel overlay.
//!
//! Displays a centered, scrollable popup showing fired alert history.
//! Alerts are shown newest-first with timestamps, rule names, and messages.

use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Maximum number of alerts kept in history.
const MAX_ALERT_HISTORY: usize = 100;

/// A single alert history entry.
#[derive(Debug, Clone)]
pub struct AlertHistoryEntry {
    /// When the alert fired.
    pub timestamp: DateTime<Utc>,
    /// The name of the alert rule that fired.
    pub rule_name: String,
    /// Human-readable message describing the alert.
    pub message: String,
}

/// Scrollable overlay showing alert history.
pub struct AlertPanel {
    /// Whether the panel is currently visible.
    visible: bool,
    /// Alert history entries (newest at the end, displayed newest-first).
    alerts: Vec<AlertHistoryEntry>,
    /// Whether alerts are muted.
    muted: bool,
    /// Scroll offset for the alert list.
    scroll: u16,
    /// Color theme.
    theme: Theme,
}

impl AlertPanel {
    /// Create a new alert panel.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            alerts: Vec::new(),
            muted: false,
            scroll: 0,
            theme,
        }
    }

    /// Whether the panel is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Toggle the panel's visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.scroll = 0;
        }
    }

    /// Set the muted state.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    /// Push a new alert into the history. Keeps at most [`MAX_ALERT_HISTORY`]
    /// entries, discarding the oldest when the limit is reached.
    pub fn push_alert(&mut self, timestamp: DateTime<Utc>, rule_name: String, message: String) {
        if self.alerts.len() >= MAX_ALERT_HISTORY {
            self.alerts.remove(0);
        }
        self.alerts.push(AlertHistoryEntry {
            timestamp,
            rule_name,
            message,
        });
    }

    /// The number of alerts currently in history.
    pub fn alert_count(&self) -> usize {
        self.alerts.len()
    }
}

impl Component for AlertPanel {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }

        match key.code {
            KeyCode::Esc => {
                self.visible = false;
                Some(Action::Noop)
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Some(Action::Noop)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Some(Action::Noop)
            }
            KeyCode::Char('m') => Some(Action::ToggleAlertMute),
            // Consume all other keys while the overlay is open.
            _ => Some(Action::Noop),
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        if !self.visible {
            return;
        }

        let popup_width = 60u16.min(area.width.saturating_sub(4));
        let popup_height = 24u16.min(area.height.saturating_sub(4));

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        frame.render_widget(Clear, popup_area);

        let muted_indicator = if self.muted { " \u{1f515} MUTED " } else { "" };
        let title = format!(" Alerts{muted_indicator} ");

        let block = Block::default()
            .title(title)
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let accent = Style::default().fg(self.theme.accent);
        let dim = Style::default().fg(self.theme.fg_dim);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        if self.alerts.is_empty() {
            lines.push(Line::from(Span::styled("  No alerts fired yet.", dim)));
        } else {
            // Show alerts newest-first.
            for entry in self.alerts.iter().rev() {
                let ts = entry.timestamp.format("%H:%M:%S").to_string();
                lines.push(Line::from(vec![
                    Span::styled(format!("  [{ts}] "), dim),
                    Span::styled(&entry.rule_name, accent),
                    Span::raw(": "),
                    Span::raw(&entry.message),
                ]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  [Esc] close  [m] mute/unmute",
            dim,
        )));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        frame.render_widget(paragraph, popup_area);
    }
}
