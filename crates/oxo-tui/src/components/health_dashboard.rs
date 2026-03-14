//! Health dashboard popup overlay.
//!
//! Displays backend connection health and throughput metrics in a centered
//! popup, similar to the [`StatsPanel`](super::stats_panel::StatsPanel).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Popup overlay showing backend health and throughput metrics.
pub struct HealthDashboard {
    visible: bool,
    theme: Theme,
    /// Display name of the active backend (e.g. "Loki", "Demo").
    pub backend_name: String,
    /// Human-readable connection state (e.g. "Connected", "Reconnecting").
    pub connection_state: String,
    /// Total number of log entries received since startup.
    pub entries_received: u64,
    /// Current ingestion rate (entries per second).
    pub entries_per_second: f64,
    /// Seconds since the application started.
    pub uptime_seconds: u64,
    /// Number of backend reconnections since startup.
    pub reconnect_count: u32,
    /// RFC 3339 timestamp of the most recently received entry (if any).
    pub last_entry_at: Option<String>,
    /// Scroll offset within the dashboard content.
    scroll: u16,
}

impl HealthDashboard {
    /// Create a new health dashboard.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            theme,
            backend_name: String::new(),
            connection_state: "Unknown".to_string(),
            entries_received: 0,
            entries_per_second: 0.0,
            uptime_seconds: 0,
            reconnect_count: 0,
            last_entry_at: None,
            scroll: 0,
        }
    }

    /// Toggle the dashboard's visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.scroll = 0;
        }
    }

    /// Whether the dashboard is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Format an uptime duration as `Xh Ym Zs`.
    fn format_uptime(seconds: u64) -> String {
        let h = seconds / 3600;
        let m = (seconds % 3600) / 60;
        let s = seconds % 60;
        if h > 0 {
            format!("{h}h {m}m {s}s")
        } else if m > 0 {
            format!("{m}m {s}s")
        } else {
            format!("{s}s")
        }
    }

    /// Build the content lines for the dashboard.
    fn build_lines(&self) -> Vec<Line<'_>> {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(self.theme.fg_dim);

        let conn_color = match self.connection_state.to_lowercase().as_str() {
            "connected" => self.theme.info,
            "reconnecting" | "connecting" => self.theme.warn,
            "disconnected" | "error" => self.theme.error,
            _ => self.theme.fg,
        };

        let mut lines = Vec::new();

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  Backend Health", bold)));
        lines.push(Line::from(""));

        // Backend name.
        lines.push(Line::from(vec![
            Span::raw("  Backend:       "),
            Span::styled(
                if self.backend_name.is_empty() {
                    "(unknown)"
                } else {
                    &self.backend_name
                },
                bold,
            ),
        ]));

        // Connection state.
        lines.push(Line::from(vec![
            Span::raw("  Connection:    "),
            Span::styled(&self.connection_state, Style::default().fg(conn_color)),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  Throughput", bold)));
        lines.push(Line::from(""));

        // Entries received.
        lines.push(Line::from(vec![
            Span::raw("  Entries:       "),
            Span::raw(format_count(self.entries_received)),
        ]));

        // Rate.
        lines.push(Line::from(vec![
            Span::raw("  Rate:          "),
            Span::raw(format!("{:.1} entries/s", self.entries_per_second)),
        ]));

        // Last entry.
        lines.push(Line::from(vec![
            Span::raw("  Last entry:    "),
            Span::styled(
                self.last_entry_at.as_deref().unwrap_or("(none)"),
                dim,
            ),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  Reliability", bold)));
        lines.push(Line::from(""));

        // Uptime.
        lines.push(Line::from(vec![
            Span::raw("  Uptime:        "),
            Span::raw(Self::format_uptime(self.uptime_seconds)),
        ]));

        // Reconnects.
        let reconnect_style = if self.reconnect_count > 0 {
            Style::default().fg(self.theme.warn)
        } else {
            Style::default().fg(self.theme.info)
        };
        lines.push(Line::from(vec![
            Span::raw("  Reconnects:    "),
            Span::styled(self.reconnect_count.to_string(), reconnect_style),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  [j/k scroll · Esc close]",
            dim,
        )));

        lines
    }
}

impl Component for HealthDashboard {
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
            // Consume all other keys to prevent fall-through.
            _ => Some(Action::Noop),
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        if !self.visible {
            return;
        }

        let popup_width = 55u16.min(area.width.saturating_sub(4));
        let popup_height = 24u16.min(area.height.saturating_sub(4));

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Health Dashboard ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let lines = self.build_lines();

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        frame.render_widget(paragraph, popup_area);
    }
}

/// Format a count with thousands separators.
fn format_count(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(*c);
    }
    result
}
