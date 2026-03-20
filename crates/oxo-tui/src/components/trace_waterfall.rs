//! Trace waterfall overlay.
//!
//! Groups log entries by trace ID and renders a timeline showing the
//! request flow across services. The left panel lists discovered traces
//! while the right panel draws a horizontal waterfall chart.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use regex::Regex;
use std::sync::LazyLock;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;
use oxo_core::LogEntry;
use oxo_core::trace::TraceDetector;

// ── Duration extraction regex ────────────────────────────────────────────

static DURATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:duration|took|latency|elapsed)[=: ]+(\d+(?:\.\d+)?)\s*(?:ms|s)").unwrap()
});

// ── Data types ───────────────────────────────────────────────────────────

/// A single span within a trace.
pub struct TraceSpan {
    /// The service or source that produced this span.
    pub service: String,
    /// Timestamp of the log entry.
    pub timestamp: DateTime<Utc>,
    /// The log line content.
    pub line: String,
    /// Log level (if detected).
    pub level: Option<String>,
    /// Duration extracted from the log line, in milliseconds.
    pub duration_ms: Option<f64>,
}

/// A group of spans sharing the same trace ID.
pub struct TraceView {
    /// The trace / request / correlation ID.
    pub trace_id: String,
    /// Spans belonging to this trace, sorted by timestamp.
    pub spans: Vec<TraceSpan>,
    /// Total wall-clock duration from first to last span (ms).
    pub total_duration_ms: f64,
}

// ── Component ────────────────────────────────────────────────────────────

/// Overlay component that visualises traces as a waterfall chart.
pub struct TraceWaterfall {
    /// Whether the overlay is visible.
    visible: bool,
    /// Color theme.
    theme: Theme,
    /// Discovered traces (each with 2+ spans).
    traces: Vec<TraceView>,
    /// Index of the currently selected trace in the left panel.
    selected_trace: usize,
    /// Scroll offset for the right-panel span list.
    scroll: u16,
}

impl TraceWaterfall {
    /// Create a new trace waterfall overlay.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            theme,
            traces: Vec::new(),
            selected_trace: 0,
            scroll: 0,
        }
    }

    /// Whether the overlay is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.scroll = 0;
        }
    }

    /// Build trace views from a slice of log entries.
    ///
    /// Uses [`TraceDetector::detect`] to extract trace IDs, groups spans by
    /// ID, sorts by timestamp, computes total durations, and keeps only
    /// traces with two or more spans.
    pub fn build_from_entries(&mut self, entries: &[LogEntry]) {
        // Group spans by trace ID.
        let mut groups: BTreeMap<String, Vec<TraceSpan>> = BTreeMap::new();

        for entry in entries {
            let Some(trace_id) = TraceDetector::detect(&entry.line) else {
                continue;
            };

            let service = entry
                .labels
                .get("service")
                .or_else(|| entry.labels.get("app"))
                .or_else(|| entry.labels.get("job"))
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());

            let level = entry.labels.get("level").cloned();

            let duration_ms = extract_duration(&entry.line);

            groups.entry(trace_id.id).or_default().push(TraceSpan {
                service,
                timestamp: entry.timestamp,
                line: entry.line.clone(),
                level,
                duration_ms,
            });
        }

        // Build TraceView list: sort spans, compute durations, filter 2+.
        let mut traces: Vec<TraceView> = groups
            .into_iter()
            .filter_map(|(id, mut spans)| {
                if spans.len() < 2 {
                    return None;
                }
                spans.sort_by_key(|s| s.timestamp);

                let first = spans.first().unwrap().timestamp;
                let last = spans.last().unwrap().timestamp;
                let total_duration_ms = (last - first).num_milliseconds().max(0) as f64;

                Some(TraceView {
                    trace_id: id,
                    spans,
                    total_duration_ms,
                })
            })
            .collect();

        // Sort traces by first span timestamp (most recent first).
        traces.sort_by(|a, b| {
            let a_ts = a.spans.first().map(|s| s.timestamp);
            let b_ts = b.spans.first().map(|s| s.timestamp);
            b_ts.cmp(&a_ts)
        });

        self.traces = traces;
        self.selected_trace = 0;
        self.scroll = 0;
    }

    // ── Rendering helpers ────────────────────────────────────────────

    /// Render the left panel: list of traces.
    fn render_trace_list(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Traces ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_unfocused));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.traces.is_empty() {
            let msg = Paragraph::new(Line::from(Span::styled(
                " No traces found.",
                Style::default().fg(self.theme.fg_dim),
            )));
            frame.render_widget(msg, inner);
            return;
        }

        let lines: Vec<Line> = self
            .traces
            .iter()
            .enumerate()
            .map(|(i, tv)| {
                let prefix = truncate_id(&tv.trace_id, 12);
                let text = format!(
                    " [{}] {} spans, {:.0}ms",
                    prefix,
                    tv.spans.len(),
                    tv.total_duration_ms,
                );
                if i == self.selected_trace {
                    Line::from(Span::styled(
                        text,
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from(Span::styled(text, Style::default().fg(self.theme.fg)))
                }
            })
            .collect();

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }

    /// Render the right panel: waterfall chart for the selected trace.
    fn render_waterfall(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Waterfall ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_unfocused));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let Some(trace) = self.traces.get(self.selected_trace) else {
            let msg = Paragraph::new(Line::from(Span::styled(
                " Select a trace from the left panel.",
                Style::default().fg(self.theme.fg_dim),
            )));
            frame.render_widget(msg, inner);
            return;
        };

        if trace.spans.is_empty() {
            return;
        }

        let bar_width = inner.width.saturating_sub(2) as usize;
        let total_ms = trace.total_duration_ms.max(1.0);
        let first_ts = trace.spans.first().unwrap().timestamp;
        // Reserve space for service label + duration + summary.
        let chart_cols = bar_width.saturating_sub(30);

        let lines: Vec<Line> = trace
            .spans
            .iter()
            .map(|span| {
                let svc = format!("{:<10}", truncate_str(&span.service, 10));

                let offset_ms = (span.timestamp - first_ts).num_milliseconds().max(0) as f64;
                let dur_ms = span.duration_ms.unwrap_or(0.0);

                // How much of the bar width this span occupies.
                let (start_col, span_cols) = if chart_cols == 0 {
                    (0, 0)
                } else {
                    let sc = ((offset_ms / total_ms) * chart_cols as f64) as usize;
                    let sp = ((dur_ms / total_ms) * chart_cols as f64).ceil() as usize;
                    (sc, sp.max(1).min(chart_cols.saturating_sub(sc)))
                };

                let mut bar = String::with_capacity(chart_cols);
                for c in 0..chart_cols {
                    if c >= start_col && c < start_col + span_cols {
                        bar.push('\u{2588}'); // full block
                    } else {
                        bar.push('\u{2591}'); // light shade
                    }
                }

                let dur_label = if dur_ms > 0.0 {
                    format!("{:.0}ms", dur_ms)
                } else {
                    "---".to_string()
                };

                let summary = truncate_str(&span.line, 30);

                let level_color = span
                    .level
                    .as_deref()
                    .and_then(|l| self.theme.log_level_color(l))
                    .unwrap_or(self.theme.fg);

                Line::from(vec![
                    Span::styled(
                        format!("[{}] ", svc),
                        Style::default().fg(self.theme.accent),
                    ),
                    Span::styled(bar, Style::default().fg(level_color)),
                    Span::styled(
                        format!(" {:>6}  ", dur_label),
                        Style::default()
                            .fg(self.theme.warn)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(summary, Style::default().fg(self.theme.fg_dim)),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));
        frame.render_widget(paragraph, inner);
    }
}

// ── Component trait ──────────────────────────────────────────────────────

impl Component for TraceWaterfall {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }

        match key.code {
            KeyCode::Esc => {
                self.visible = false;
                Some(Action::Noop)
            }
            // Scroll spans in the waterfall (right panel).
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Some(Action::Noop)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Some(Action::Noop)
            }
            // Switch selected trace (left panel).
            KeyCode::Tab => {
                if !self.traces.is_empty() {
                    self.selected_trace = (self.selected_trace + 1) % self.traces.len();
                    self.scroll = 0;
                }
                Some(Action::Noop)
            }
            KeyCode::BackTab => {
                if !self.traces.is_empty() {
                    self.selected_trace = if self.selected_trace == 0 {
                        self.traces.len() - 1
                    } else {
                        self.selected_trace - 1
                    };
                    self.scroll = 0;
                }
                Some(Action::Noop)
            }
            // Consume all other keys while visible.
            _ => Some(Action::Noop),
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        if !self.visible {
            return;
        }

        // 80% of screen, centered.
        let popup_width = ((area.width as u32 * 80 / 100) as u16).max(50);
        let popup_height = ((area.height as u32 * 80 / 100) as u16).max(16);

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Trace Waterfall ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        if inner.height < 4 || inner.width < 20 {
            return;
        }

        // Split inner: content + footer.
        let chunks = Layout::vertical([
            Constraint::Min(1),    // Content
            Constraint::Length(1), // Footer
        ])
        .split(inner);

        let content_area = chunks[0];
        let footer_area = chunks[1];

        // Split content into left (30%) and right (70%).
        let panels = Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(content_area);

        self.render_trace_list(frame, panels[0]);
        self.render_waterfall(frame, panels[1]);

        // Footer.
        let dim = Style::default().fg(self.theme.fg_dim);
        let footer = Paragraph::new(Line::from(Span::styled(
            " [Tab/S-Tab] switch trace  [j/k] scroll  [Esc] close",
            dim,
        )));
        frame.render_widget(footer, footer_area);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Extract a duration in milliseconds from a log line.
///
/// Matches patterns like `duration=45ms`, `took: 120.5 ms`, `latency=2s`,
/// `elapsed: 300ms`, etc.
fn extract_duration(line: &str) -> Option<f64> {
    let caps = DURATION_RE.captures(line)?;
    let value: f64 = caps[1].parse().ok()?;
    // Check if the unit is seconds.
    let full_match = caps.get(0)?.as_str();
    if full_match.ends_with('s') && !full_match.ends_with("ms") {
        Some(value * 1000.0)
    } else {
        Some(value)
    }
}

/// Truncate a trace ID for display, showing the first N characters.
fn truncate_id(id: &str, max_len: usize) -> String {
    if id.len() <= max_len {
        id.to_string()
    } else {
        format!("{}\u{2026}", &id[..max_len.saturating_sub(1)])
    }
}

/// Truncate a string, appending an ellipsis if needed.
fn truncate_str(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = chars[..max_len.saturating_sub(1)].iter().collect();
        format!("{truncated}\u{2026}")
    }
}
