//! Log detail/inspect panel component.
//!
//! Shows the full content of a selected log entry: all labels, the complete
//! log line (unwrapped), and the raw JSON response (if available). Renders
//! as a right-side split or overlay.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use oxo_core::LogEntry;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Detail panel for inspecting a single log entry.
pub struct DetailPanel {
    /// Whether the panel is visible.
    visible: bool,
    /// The log entry being inspected.
    entry: Option<LogEntry>,
    /// Scroll offset within the detail panel.
    scroll: u16,
    /// Color theme.
    theme: Theme,
}

impl DetailPanel {
    /// Create a new detail panel.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            entry: None,
            scroll: 0,
            theme,
        }
    }

    /// Whether the panel is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Toggle visibility. Sets the entry to inspect.
    pub fn toggle(&mut self, entry: Option<LogEntry>) {
        if self.visible {
            self.visible = false;
            self.entry = None;
            self.scroll = 0;
        } else if let Some(e) = entry {
            self.visible = true;
            self.entry = Some(e);
            self.scroll = 0;
        }
    }

    /// Scroll the detail panel.
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    /// Scroll the detail panel up.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}

impl Component for DetailPanel {
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }
        match key.code {
            crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Enter => {
                self.visible = false;
                self.entry = None;
                self.scroll = 0;
                Some(Action::Noop)
            }
            crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                self.scroll_down();
                Some(Action::Noop)
            }
            crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                self.scroll_up();
                Some(Action::Noop)
            }
            _ => Some(Action::Noop), // Consume all keys when visible.
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        if !self.visible {
            return;
        }

        let Some(ref entry) = self.entry else {
            return;
        };

        // Take right 60% of screen as a side panel.
        let panel_width = (area.width * 3 / 5)
            .max(40)
            .min(area.width.saturating_sub(4));
        let horizontal = Layout::horizontal([Constraint::Length(panel_width)]).flex(Flex::End);
        let [panel_area] = horizontal.areas(area);

        frame.render_widget(Clear, panel_area);

        let block = Block::default()
            .title(" Log Detail ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let mut lines: Vec<Line> = vec![
            // Timestamp.
            Line::from(vec![
                Span::styled("Timestamp: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(entry.timestamp.to_rfc3339()),
            ]),
            Line::from(""),
        ];

        // Labels.
        lines.push(Line::from(Span::styled(
            "Labels",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )));
        lines.push(Line::from(""));
        for (key, value) in &entry.labels {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key}: "), Style::default().fg(self.theme.accent)),
                Span::raw(value),
            ]));
        }

        lines.push(Line::from(""));

        // Log line.
        lines.push(Line::from(Span::styled(
            "Message",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )));
        lines.push(Line::from(""));
        // Split long lines for readability.
        for chunk in entry
            .line
            .as_bytes()
            .chunks(panel_width.saturating_sub(4) as usize)
        {
            if let Ok(s) = std::str::from_utf8(chunk) {
                lines.push(Line::from(format!("  {s}")));
            }
        }

        // Raw JSON (if available).
        if let Some(ref raw) = entry.raw {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Raw JSON",
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            lines.push(Line::from(""));
            if let Ok(pretty) = serde_json::to_string_pretty(raw) {
                for json_line in pretty.lines() {
                    lines.push(Line::from(format!("  {json_line}")));
                }
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  [Esc/Enter to close, j/k to scroll]",
            self.theme.dimmed(),
        )));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        frame.render_widget(paragraph, panel_area);
    }
}
