//! Log rate sparkline component.
//!
//! Displays a rolling sparkline chart showing the rate of incoming log
//! entries over time. This gives users an at-a-glance view of log volume
//! and helps identify spikes or drops.

use std::collections::VecDeque;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Sparkline as RatatuiSparkline};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Number of time buckets to display in the sparkline.
const SPARKLINE_WIDTH: usize = 60;

/// Sparkline component state.
pub struct SparklineChart {
    /// Rolling window of log counts per tick interval.
    data: VecDeque<u64>,
    /// Number of entries received in the current tick.
    current_tick_count: u64,
    /// The most recent rate (entries per tick).
    current_rate: u64,
    /// Color theme.
    theme: Theme,
}

impl SparklineChart {
    /// Create a new sparkline chart.
    pub fn new(theme: Theme) -> Self {
        let mut data = VecDeque::with_capacity(SPARKLINE_WIDTH);
        data.resize(SPARKLINE_WIDTH, 0);

        Self {
            data,
            current_tick_count: 0,
            current_rate: 0,
            theme,
        }
    }

    /// Record that N new log entries arrived.
    ///
    /// Call this each time a batch of entries is added to the log buffer.
    pub fn record_entries(&mut self, count: u64) {
        self.current_tick_count += count;
    }

    /// Advance the sparkline by one tick.
    ///
    /// Pushes the accumulated count for this interval and resets the counter.
    /// Call this on each tick event.
    pub fn tick(&mut self) {
        self.current_rate = self.current_tick_count;

        if self.data.len() >= SPARKLINE_WIDTH {
            self.data.pop_front();
        }
        self.data.push_back(self.current_tick_count);
        self.current_tick_count = 0;
    }

    /// Get the current log rate (entries per tick interval).
    pub fn current_rate(&self) -> u64 {
        self.current_rate
    }
}

impl Component for SparklineChart {
    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        if matches!(action, Action::Tick) {
            self.tick();
        }
        None
    }

    fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let border_style = if focused {
            self.theme.border_focused()
        } else {
            self.theme.border_unfocused()
        };

        let title = format!(" Rate: {}/tick ", self.current_rate);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let data: Vec<u64> = self.data.iter().copied().collect();
        let sparkline = RatatuiSparkline::default()
            .block(block)
            .data(&data)
            .style(ratatui::style::Style::default().fg(self.theme.sparkline));

        frame.render_widget(sparkline, area);
    }
}
