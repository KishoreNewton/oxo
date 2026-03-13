//! Status bar component.
//!
//! Displays connection status, current backend name, log throughput,
//! buffer usage, and the active query at the bottom of the screen.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Connection state for display purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Successfully connected.
    Connected,
    /// Not connected / disconnected.
    Disconnected,
    /// Attempting to reconnect.
    Reconnecting,
}

/// Status bar component state.
pub struct StatusBar {
    /// Name of the active backend (e.g. "Loki").
    backend_name: String,
    /// Current connection state.
    connection_state: ConnectionState,
    /// Current log rate (entries per tick).
    rate: u64,
    /// Current buffer size.
    buffer_size: usize,
    /// Maximum buffer capacity.
    max_buffer_size: usize,
    /// Color theme.
    theme: Theme,
}

impl StatusBar {
    /// Create a new status bar.
    pub fn new(theme: Theme, backend_name: String, max_buffer_size: usize) -> Self {
        Self {
            backend_name,
            connection_state: ConnectionState::Disconnected,
            rate: 0,
            buffer_size: 0,
            max_buffer_size,
            theme,
        }
    }

    /// Update the connection state.
    pub fn set_connection_state(&mut self, state: ConnectionState) {
        self.connection_state = state;
    }

    /// Update the log rate display.
    pub fn set_rate(&mut self, rate: u64) {
        self.rate = rate;
    }

    /// Update the buffer size display.
    pub fn set_buffer_size(&mut self, size: usize) {
        self.buffer_size = size;
    }

    /// Format a number with K/M suffixes for compact display.
    fn compact_number(n: usize) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}K", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    }
}

impl Component for StatusBar {
    fn handle_action(&mut self, _action: &Action) -> Option<Action> {
        None
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        let (state_symbol, state_text) = match self.connection_state {
            ConnectionState::Connected => ("●", "connected"),
            ConnectionState::Disconnected => ("○", "disconnected"),
            ConnectionState::Reconnecting => ("◌", "reconnecting"),
        };

        let state_color = match self.connection_state {
            ConnectionState::Connected => self.theme.info,
            ConnectionState::Disconnected => self.theme.error,
            ConnectionState::Reconnecting => self.theme.warn,
        };

        let left = Line::from(vec![
            Span::styled(
                format!(" {state_symbol} "),
                ratatui::style::Style::default().fg(state_color),
            ),
            Span::styled(
                format!("{} {state_text}", self.backend_name),
                self.theme.status_bar(),
            ),
            Span::styled(" │ ", self.theme.status_bar()),
            Span::styled(format!("{}/tick", self.rate), self.theme.status_bar()),
            Span::styled(" │ ", self.theme.status_bar()),
            Span::styled(
                format!(
                    "buf: {}/{}",
                    Self::compact_number(self.buffer_size),
                    Self::compact_number(self.max_buffer_size),
                ),
                self.theme.status_bar(),
            ),
        ]);

        let right_text = " q:quit  /:query  f:filter  ?:help ";

        // Pad the line to fill the full width.
        let left_len: usize = left.spans.iter().map(|s| s.content.len()).sum();
        let padding = area.width as usize
            - left_len.min(area.width as usize)
            - right_text.len().min(area.width as usize);

        let line = Line::from(
            left.spans
                .into_iter()
                .chain(std::iter::once(Span::styled(
                    " ".repeat(padding),
                    self.theme.status_bar(),
                )))
                .chain(std::iter::once(Span::styled(
                    right_text,
                    self.theme.status_bar(),
                )))
                .collect::<Vec<_>>(),
        );

        let paragraph = Paragraph::new(line)
            .style(self.theme.status_bar())
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, area);
    }
}
