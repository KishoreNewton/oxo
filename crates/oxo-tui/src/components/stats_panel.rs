//! Log statistics popup overlay.
//!
//! Displays aggregate statistics computed from the current log buffer:
//! total entries, breakdown by level, top sources, and error rate.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use oxo_core::LogEntry;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Statistics computed from the current log buffer.
#[derive(Default)]
pub struct LogStats {
    /// Total number of entries in the buffer.
    pub total: usize,
    /// Counts per log level, sorted by count descending.
    pub by_level: Vec<(String, usize)>,
    /// Top 5 services/sources by entry count.
    pub top_sources: Vec<(String, usize)>,
    /// Error rate as a percentage (errors + fatals / total * 100).
    pub error_rate: f64,
}

/// Popup overlay that shows log statistics for the current buffer.
pub struct StatsPanel {
    visible: bool,
    stats: LogStats,
    theme: Theme,
}

impl StatsPanel {
    /// Create a new statistics panel.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            stats: LogStats::default(),
            theme,
        }
    }

    /// Toggle the panel's visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Whether the panel is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Recompute statistics from the given buffer slice.
    pub fn update_stats(&mut self, entries: &[LogEntry]) {
        let total = entries.len();
        if total == 0 {
            self.stats = LogStats::default();
            return;
        }

        let mut level_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut source_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for entry in entries {
            // Count by level label.
            let level = entry
                .labels
                .get("level")
                .or_else(|| entry.labels.get("severity"))
                .or_else(|| entry.labels.get("lvl"))
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            *level_counts.entry(level.to_lowercase()).or_insert(0) += 1;

            // Count by service/source label.
            let source = entry
                .labels
                .get("service")
                .or_else(|| entry.labels.get("app"))
                .or_else(|| entry.labels.get("job"))
                .or_else(|| entry.labels.get("container"))
                .or_else(|| entry.labels.get("source"))
                .or_else(|| {
                    // Fallback: first label that isn't "level"/"severity"/"lvl".
                    entry.labels.iter().find_map(|(k, v)| {
                        if !matches!(k.as_str(), "level" | "severity" | "lvl") {
                            Some(v)
                        } else {
                            None
                        }
                    })
                })
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            *source_counts.entry(source).or_insert(0) += 1;
        }

        // Calculate error rate.
        let error_count = level_counts
            .iter()
            .filter(|(k, _)| matches!(k.as_str(), "error" | "err" | "fatal" | "critical"))
            .map(|(_, v)| v)
            .sum::<usize>();
        let error_rate = if total > 0 {
            error_count as f64 / total as f64 * 100.0
        } else {
            0.0
        };

        // Sort levels by count descending.
        let mut by_level: Vec<(String, usize)> = level_counts.into_iter().collect();
        by_level.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Top 5 sources.
        let mut sources: Vec<(String, usize)> = source_counts.into_iter().collect();
        sources.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        sources.truncate(5);

        self.stats = LogStats {
            total,
            by_level,
            top_sources: sources,
            error_rate,
        };
    }
}

impl Component for StatsPanel {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }
        match key.code {
            KeyCode::Esc => {
                self.visible = false;
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

        let popup_width = 50u16.min(area.width.saturating_sub(4));
        let popup_height = 24u16.min(area.height.saturating_sub(4));

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Log Statistics ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let bold = Style::default().add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(self.theme.fg_dim);

        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  Total entries: "),
            Span::styled(
                format_count(self.stats.total),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  Error rate:    "),
            Span::styled(
                format!("{:.1}%", self.stats.error_rate),
                if self.stats.error_rate > 5.0 {
                    Style::default()
                        .fg(self.theme.error)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.info)
                },
            ),
        ]));
        lines.push(Line::from(""));

        // By level section.
        lines.push(Line::from(Span::styled("  By Level", bold)));
        if self.stats.by_level.is_empty() {
            lines.push(Line::from(Span::styled("    (no data)", dim)));
        } else {
            for (level, count) in &self.stats.by_level {
                let pct = if self.stats.total > 0 {
                    *count as f64 / self.stats.total as f64 * 100.0
                } else {
                    0.0
                };
                let level_style = self.theme.log_level_style(level);
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(format!("{:<8}", level), level_style),
                    Span::raw(format!("{:>7}", format_count(*count))),
                    Span::styled(format!("  ({:.1}%)", pct), dim),
                ]));
            }
        }
        lines.push(Line::from(""));

        // Top sources section.
        lines.push(Line::from(Span::styled("  Top Sources", bold)));
        if self.stats.top_sources.is_empty() {
            lines.push(Line::from(Span::styled("    (no data)", dim)));
        } else {
            for (source, count) in &self.stats.top_sources {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::raw(format!("{:<20}", truncate(source, 20))),
                    Span::raw(format!("{:>7}", format_count(*count))),
                ]));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  [Esc to close]", dim)));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, popup_area);
    }
}

/// Format a count with thousands separators.
fn format_count(n: usize) -> String {
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

/// Truncate a string to at most `max_len` characters, appending "…" if cut.
fn truncate(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = chars[..max_len.saturating_sub(1)].iter().collect();
        format!("{truncated}…")
    }
}
