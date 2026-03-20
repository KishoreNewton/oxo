//! Analytics dashboard overlay.
//!
//! A multi-tab overlay with 5 sub-views: Patterns, Anomalies, Correlations,
//! Trends, and Top-N. The panel holds simple data types that the app populates
//! -- it does not depend directly on `oxo-analytics` types.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Sparkline, Tabs, Wrap};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Number of tabs in the analytics panel.
const TAB_COUNT: usize = 5;

// ── Data types ───────────────────────────────────────────────────────────

/// Information about a detected log pattern / template.
#[derive(Debug, Clone)]
pub struct PatternInfo {
    /// The pattern template (e.g. "Connection from {*} closed").
    pub template: String,
    /// How many log lines matched this pattern.
    pub count: usize,
    /// An example raw log line matching this pattern.
    pub example: String,
}

/// Severity level for an anomaly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalySeverity {
    /// A volume spike was detected.
    VolumeSpike,
    /// A previously-unseen pattern appeared.
    NewPattern,
}

/// A detected anomaly.
#[derive(Debug, Clone)]
pub struct AnomalyInfo {
    /// Human-readable description of the anomaly.
    pub description: String,
    /// When the anomaly was detected (formatted string).
    pub timestamp: String,
    /// Severity classification.
    pub severity: AnomalySeverity,
}

/// A correlation between a label value and error spikes.
#[derive(Debug, Clone)]
pub struct CorrelationInfo {
    /// Label or dimension name.
    pub label: String,
    /// Label value.
    pub value: String,
    /// Baseline rate/count.
    pub baseline: f64,
    /// Current rate/count.
    pub current: f64,
    /// Change factor (current / baseline).
    pub change: f64,
}

/// Information about a slow endpoint.
#[derive(Debug, Clone)]
pub struct EndpointInfo {
    /// URL pattern or route.
    pub pattern: String,
    /// 50th percentile latency (ms).
    pub p50: f64,
    /// 95th percentile latency (ms).
    pub p95: f64,
    /// 99th percentile latency (ms).
    pub p99: f64,
    /// Total request count.
    pub count: usize,
}

// ── Panel ────────────────────────────────────────────────────────────────

/// Multi-tab analytics dashboard overlay.
pub struct AnalyticsPanel {
    /// Whether the panel is currently visible.
    visible: bool,
    /// Index of the active tab (0-4).
    active_tab: usize,
    /// Scroll offset within the active tab's content.
    scroll: u16,
    /// Color theme.
    theme: Theme,

    // ── Data fields (populated by app) ───────────────────────────────
    /// Detected log patterns.
    pub patterns: Vec<PatternInfo>,
    /// Detected anomalies.
    pub anomalies: Vec<AnomalyInfo>,
    /// Label-value correlations with error spikes.
    pub correlations: Vec<CorrelationInfo>,
    /// Human-readable trend description.
    pub trend_description: String,
    /// Trend data points for sparkline display.
    pub trend_data: Vec<f64>,
    /// Slowest endpoints.
    pub endpoints: Vec<EndpointInfo>,
    /// Noisiest sources: (name, count, percentage).
    pub noisy_sources: Vec<(String, usize, f64)>,
}

impl AnalyticsPanel {
    /// Create a new analytics panel.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            active_tab: 0,
            scroll: 0,
            theme,
            patterns: Vec::new(),
            anomalies: Vec::new(),
            correlations: Vec::new(),
            trend_description: String::new(),
            trend_data: Vec::new(),
            endpoints: Vec::new(),
            noisy_sources: Vec::new(),
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

    // ── Setters ──────────────────────────────────────────────────────

    /// Replace the patterns data.
    pub fn set_patterns(&mut self, patterns: Vec<PatternInfo>) {
        self.patterns = patterns;
    }

    /// Replace the anomalies data.
    pub fn set_anomalies(&mut self, anomalies: Vec<AnomalyInfo>) {
        self.anomalies = anomalies;
    }

    /// Replace the correlations data.
    pub fn set_correlations(&mut self, correlations: Vec<CorrelationInfo>) {
        self.correlations = correlations;
    }

    /// Replace the trend description and sparkline data.
    pub fn set_trend(&mut self, description: String, data: Vec<f64>) {
        self.trend_description = description;
        self.trend_data = data;
    }

    /// Replace the endpoints data.
    pub fn set_endpoints(&mut self, endpoints: Vec<EndpointInfo>) {
        self.endpoints = endpoints;
    }

    /// Replace the noisy sources data.
    pub fn set_noisy_sources(&mut self, noisy_sources: Vec<(String, usize, f64)>) {
        self.noisy_sources = noisy_sources;
    }

    // ── Tab switching helpers ────────────────────────────────────────

    fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % TAB_COUNT;
        self.scroll = 0;
    }

    fn prev_tab(&mut self) {
        self.active_tab = if self.active_tab == 0 {
            TAB_COUNT - 1
        } else {
            self.active_tab - 1
        };
        self.scroll = 0;
    }

    fn go_to_tab(&mut self, tab: usize) {
        if tab < TAB_COUNT {
            self.active_tab = tab;
            self.scroll = 0;
        }
    }

    // ── Rendering helpers ────────────────────────────────────────────

    fn render_patterns(&self) -> Vec<Line<'_>> {
        let accent = Style::default().fg(self.theme.accent);
        let dim = Style::default().fg(self.theme.fg_dim);
        let count_style = Style::default()
            .fg(self.theme.warn)
            .add_modifier(Modifier::BOLD);

        let mut lines = Vec::new();

        if self.patterns.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  No patterns detected yet.", dim)));
            return lines;
        }

        for (i, pattern) in self.patterns.iter().enumerate() {
            lines.push(Line::from(""));
            // Render template with {*} placeholders highlighted in cyan.
            let mut spans = vec![Span::raw(format!("  #{:<3} ", i + 1))];
            for part in pattern.template.split("{*}") {
                if !spans.is_empty()
                    && spans
                        .last()
                        .is_some_and(|s: &Span| s.content.ends_with('}'))
                {
                    // This is after a {*} -- insert the placeholder span first.
                }
                spans.push(Span::raw(part.to_string()));
                spans.push(Span::styled("{*}", accent));
            }
            // Remove the trailing {*} that was added after the last split part.
            if spans.last().is_some_and(|s| s.content == "{*}") {
                spans.pop();
            }
            lines.push(Line::from(spans));

            lines.push(Line::from(vec![
                Span::raw("       count: "),
                Span::styled(format!("{}", pattern.count), count_style),
            ]));
            lines.push(Line::from(vec![
                Span::raw("       ex:    "),
                Span::styled(truncate(&pattern.example, 60), dim),
            ]));
        }

        lines
    }

    fn render_anomalies(&self) -> Vec<Line<'_>> {
        let dim = Style::default().fg(self.theme.fg_dim);

        let mut lines = Vec::new();

        if self.anomalies.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  No anomalies detected.", dim)));
            return lines;
        }

        for anomaly in &self.anomalies {
            let severity_style = match anomaly.severity {
                AnomalySeverity::VolumeSpike => Style::default()
                    .fg(self.theme.error)
                    .add_modifier(Modifier::BOLD),
                AnomalySeverity::NewPattern => Style::default()
                    .fg(self.theme.warn)
                    .add_modifier(Modifier::BOLD),
            };
            let severity_label = match anomaly.severity {
                AnomalySeverity::VolumeSpike => "SPIKE",
                AnomalySeverity::NewPattern => "NEW  ",
            };

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(format!("  [{severity_label}]"), severity_style),
                Span::styled(format!(" {} ", anomaly.timestamp), dim),
                Span::raw(&anomaly.description),
            ]));
        }

        lines
    }

    fn render_correlations(&self) -> Vec<Line<'_>> {
        let dim = Style::default().fg(self.theme.fg_dim);
        let bold = Style::default().add_modifier(Modifier::BOLD);

        let mut lines = Vec::new();

        if self.correlations.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  No correlations found.", dim)));
            return lines;
        }

        // Header row.
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            format!(
                "  {:<16} {:<14} {:>10} {:>10} {:>8}",
                "Label", "Value", "Baseline", "Current", "Change"
            ),
            bold,
        )]));
        lines.push(Line::from(Span::styled(
            format!("  {}", "-".repeat(62)),
            dim,
        )));

        for corr in &self.correlations {
            let change_style = if corr.change > 1.5 {
                Style::default()
                    .fg(self.theme.error)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.fg)
            };

            lines.push(Line::from(vec![
                Span::raw(format!(
                    "  {:<16} {:<14} {:>10.2} {:>10.2} ",
                    truncate(&corr.label, 16),
                    truncate(&corr.value, 14),
                    corr.baseline,
                    corr.current,
                )),
                Span::styled(format!("{:>7.2}x", corr.change), change_style),
            ]));
        }

        lines
    }

    fn render_trends(&self) -> Vec<Line<'_>> {
        let dim = Style::default().fg(self.theme.fg_dim);
        let bold = Style::default().add_modifier(Modifier::BOLD);

        let mut lines = Vec::new();
        lines.push(Line::from(""));

        if self.trend_description.is_empty() && self.trend_data.is_empty() {
            lines.push(Line::from(Span::styled("  No trend data available.", dim)));
            return lines;
        }

        lines.push(Line::from(Span::styled(
            format!("  {}", &self.trend_description),
            bold,
        )));
        lines.push(Line::from(""));

        // The sparkline will be rendered separately in the render method,
        // so we add a placeholder note here for the text portion.
        if !self.trend_data.is_empty() {
            lines.push(Line::from(Span::styled("  Error rate over time:", dim)));
            // Sparkline is rendered as a widget below this text area.
        }

        lines
    }

    fn render_topn(&self) -> Vec<Line<'_>> {
        let dim = Style::default().fg(self.theme.fg_dim);
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let accent = Style::default().fg(self.theme.accent);

        let mut lines = Vec::new();

        // Slowest endpoints section.
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  Slowest Endpoints", accent)));
        lines.push(Line::from(""));

        if self.endpoints.is_empty() {
            lines.push(Line::from(Span::styled("  (no endpoint data)", dim)));
        } else {
            lines.push(Line::from(Span::styled(
                format!(
                    "  {:<30} {:>8} {:>8} {:>8} {:>6}",
                    "Pattern", "p50", "p95", "p99", "Count"
                ),
                bold,
            )));
            lines.push(Line::from(Span::styled(
                format!("  {}", "-".repeat(66)),
                dim,
            )));

            for ep in &self.endpoints {
                let p99_style = if ep.p99 > 1000.0 {
                    Style::default()
                        .fg(self.theme.error)
                        .add_modifier(Modifier::BOLD)
                } else if ep.p99 > 500.0 {
                    Style::default().fg(self.theme.warn)
                } else {
                    Style::default().fg(self.theme.fg)
                };

                lines.push(Line::from(vec![
                    Span::raw(format!(
                        "  {:<30} {:>7.1} {:>7.1} ",
                        truncate(&ep.pattern, 30),
                        ep.p50,
                        ep.p95,
                    )),
                    Span::styled(format!("{:>7.1}", ep.p99), p99_style),
                    Span::raw(format!(" {:>6}", ep.count)),
                ]));
            }
        }

        // Noisiest sources section.
        lines.push(Line::from(""));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  Noisiest Sources", accent)));
        lines.push(Line::from(""));

        if self.noisy_sources.is_empty() {
            lines.push(Line::from(Span::styled("  (no source data)", dim)));
        } else {
            lines.push(Line::from(Span::styled(
                format!("  {:<30} {:>10} {:>8}", "Source", "Count", "%"),
                bold,
            )));
            lines.push(Line::from(Span::styled(
                format!("  {}", "-".repeat(50)),
                dim,
            )));

            for (name, count, pct) in &self.noisy_sources {
                lines.push(Line::from(format!(
                    "  {:<30} {:>10} {:>7.1}%",
                    truncate(name, 30),
                    count,
                    pct,
                )));
            }
        }

        lines
    }
}

impl Component for AnalyticsPanel {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }

        match key.code {
            KeyCode::Esc => {
                self.visible = false;
                Some(Action::Noop)
            }
            // Tab switching.
            KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => {
                self.next_tab();
                Some(Action::Noop)
            }
            KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => {
                self.prev_tab();
                Some(Action::Noop)
            }
            KeyCode::Char(c @ '1'..='5') => {
                let n = (c as u8 - b'1') as usize;
                self.go_to_tab(n);
                Some(Action::Noop)
            }
            // Scrolling.
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                Some(Action::Noop)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
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
        let popup_width = ((area.width as u32 * 80 / 100) as u16).max(40);
        let popup_height = ((area.height as u32 * 80 / 100) as u16).max(16);

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Analytics ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        if inner.height < 4 || inner.width < 10 {
            return;
        }

        // Split inner area into tabs bar, content area, and footer.
        let chunks = Layout::vertical([
            Constraint::Length(2), // Tabs
            Constraint::Min(1),    // Content
            Constraint::Length(1), // Footer
        ])
        .split(inner);

        let tabs_area = chunks[0];
        let content_area = chunks[1];
        let footer_area = chunks[2];

        // ── Tabs ─────────────────────────────────────────────────────
        let tab_titles: Vec<Line> = vec![
            Line::from(" Patterns "),
            Line::from(" Anomalies "),
            Line::from(" Correlations "),
            Line::from(" Trends "),
            Line::from(" Top-N "),
        ];

        let tabs = Tabs::new(tab_titles)
            .select(self.active_tab)
            .highlight_style(
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
            .style(Style::default().fg(self.theme.fg_dim))
            .divider(Span::raw(" | "));

        frame.render_widget(tabs, tabs_area);

        // ── Content ──────────────────────────────────────────────────
        match self.active_tab {
            0 => {
                let lines = self.render_patterns();
                let paragraph = Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0));
                frame.render_widget(paragraph, content_area);
            }
            1 => {
                let lines = self.render_anomalies();
                let paragraph = Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0));
                frame.render_widget(paragraph, content_area);
            }
            2 => {
                let lines = self.render_correlations();
                let paragraph = Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0));
                frame.render_widget(paragraph, content_area);
            }
            3 => {
                // Trends: text description + sparkline below.
                let lines = self.render_trends();
                let text_height = lines.len() as u16;

                if content_area.height > text_height + 3 && !self.trend_data.is_empty() {
                    let trend_chunks =
                        Layout::vertical([Constraint::Length(text_height), Constraint::Min(3)])
                            .split(content_area);

                    let paragraph = Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .scroll((self.scroll, 0));
                    frame.render_widget(paragraph, trend_chunks[0]);

                    // Convert f64 data to u64 for Sparkline widget.
                    let spark_data: Vec<u64> = self
                        .trend_data
                        .iter()
                        .map(|v| (*v * 100.0) as u64)
                        .collect();

                    let sparkline_block = Block::default()
                        .borders(Borders::TOP)
                        .border_style(Style::default().fg(self.theme.fg_dim));

                    let sparkline = Sparkline::default()
                        .block(sparkline_block)
                        .data(&spark_data)
                        .style(Style::default().fg(self.theme.sparkline));

                    frame.render_widget(sparkline, trend_chunks[1]);
                } else {
                    let paragraph = Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .scroll((self.scroll, 0));
                    frame.render_widget(paragraph, content_area);
                }
            }
            4 => {
                let lines = self.render_topn();
                let paragraph = Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0));
                frame.render_widget(paragraph, content_area);
            }
            _ => {}
        }

        // ── Footer ───────────────────────────────────────────────────
        let dim = Style::default().fg(self.theme.fg_dim);
        let footer = Paragraph::new(Line::from(Span::styled(
            " [1-5] switch tab  [j/k] scroll  [Esc] close",
            dim,
        )));
        frame.render_widget(footer, footer_area);
    }
}

/// Truncate a string to at most `max_len` characters, appending "..." if cut.
fn truncate(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = chars[..max_len.saturating_sub(1)].iter().collect();
        format!("{truncated}\u{2026}")
    }
}
