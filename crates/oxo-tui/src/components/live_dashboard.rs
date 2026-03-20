//! Live metrics dashboard overlay.
//!
//! Displays real-time metrics extracted from log entries: request rate,
//! error rate, latency percentiles, and top endpoints. Data is fed from
//! the analytics engine's periodic snapshots.
//!
//! Keybinding: `L` in normal mode.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Bar, BarChart, BarGroup, Block, Borders, Clear, Gauge, Paragraph, Sparkline, Tabs,
};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// A single metric time series data point.
#[derive(Debug, Clone)]
pub struct MetricPoint {
    pub timestamp: String,
    pub value: f64,
}

/// A named metric with its time series.
#[derive(Debug, Clone)]
pub struct DashboardMetric {
    pub name: String,
    pub unit: String,
    pub current: f64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub series: Vec<u64>, // sparkline-compatible values
}

/// Dashboard layout configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardTab {
    Overview,
    Rates,
    Latency,
    Errors,
}

const TABS: &[&str] = &["Overview", "Rates", "Latency", "Errors"];

/// Live metrics dashboard component.
pub struct LiveDashboard {
    visible: bool,
    active_tab: DashboardTab,
    theme: Theme,

    // Metrics data (populated by app from analytics snapshots).
    pub request_rate: DashboardMetric,
    pub error_rate: DashboardMetric,
    pub p50_latency: DashboardMetric,
    pub p95_latency: DashboardMetric,
    pub p99_latency: DashboardMetric,
    pub log_volume: DashboardMetric,
    pub error_ratio: f64, // 0.0 - 1.0
    pub top_endpoints: Vec<(String, u64)>,
    pub status_distribution: Vec<(String, u64)>, // e.g. [("2xx", 850), ("4xx", 120), ("5xx", 30)]
}

impl LiveDashboard {
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            active_tab: DashboardTab::Overview,
            theme,
            request_rate: empty_metric("Requests/s", "req/s"),
            error_rate: empty_metric("Errors/s", "err/s"),
            p50_latency: empty_metric("P50 Latency", "ms"),
            p95_latency: empty_metric("P95 Latency", "ms"),
            p99_latency: empty_metric("P99 Latency", "ms"),
            log_volume: empty_metric("Log Volume", "lines/s"),
            error_ratio: 0.0,
            top_endpoints: Vec::new(),
            status_distribution: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Center the overlay in the terminal.
    fn overlay_area(&self, area: Rect) -> Rect {
        let h = (area.height as f32 * 0.85) as u16;
        let w = (area.width as f32 * 0.90) as u16;
        let vertical = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(h),
            Constraint::Fill(1),
        ]);
        let horizontal = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(w),
            Constraint::Fill(1),
        ]);
        let [_, mid, _] = vertical.areas(area);
        let [_, center, _] = horizontal.areas(mid);
        center
    }

    fn render_overview(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(5), // KPI cards
            Constraint::Length(8), // Sparklines
            Constraint::Min(5),    // Status distribution
        ])
        .split(area);

        // KPI cards row
        let kpi_cols = Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(chunks[0]);

        self.render_kpi(
            frame,
            kpi_cols[0],
            "Req/s",
            self.request_rate.current,
            Color::Blue,
        );
        self.render_kpi(
            frame,
            kpi_cols[1],
            "Err/s",
            self.error_rate.current,
            Color::Red,
        );
        self.render_kpi(
            frame,
            kpi_cols[2],
            "P50 (ms)",
            self.p50_latency.current,
            Color::Yellow,
        );
        self.render_kpi(
            frame,
            kpi_cols[3],
            "P95 (ms)",
            self.p95_latency.current,
            Color::Magenta,
        );

        // Sparklines row
        let spark_cols =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

        self.render_sparkline(
            frame,
            spark_cols[0],
            "Request Rate",
            &self.request_rate.series,
            Color::Blue,
        );
        self.render_sparkline(
            frame,
            spark_cols[1],
            "Error Rate",
            &self.error_rate.series,
            Color::Red,
        );

        // Error ratio gauge + top endpoints
        let bottom_cols =
            Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(chunks[2]);

        let ratio_pct = (self.error_ratio * 100.0).min(100.0) as u16;
        let gauge_color = if ratio_pct > 10 {
            Color::Red
        } else if ratio_pct > 5 {
            Color::Yellow
        } else {
            Color::Green
        };
        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("Error Ratio"))
            .gauge_style(Style::default().fg(gauge_color))
            .ratio(self.error_ratio.min(1.0));
        frame.render_widget(gauge, bottom_cols[0]);

        // Top endpoints
        let endpoints: Vec<Line> = self
            .top_endpoints
            .iter()
            .take(bottom_cols[1].height.saturating_sub(2) as usize)
            .enumerate()
            .map(|(i, (ep, count))| {
                Line::from(vec![
                    Span::styled(
                        format!("  {}. ", i + 1),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(format!("{ep:<40}"), Style::default().fg(self.theme.fg)),
                    Span::styled(
                        format!("{count:>8}"),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            })
            .collect();

        let ep_block = Block::default()
            .borders(Borders::ALL)
            .title("Top Endpoints");
        let ep_para = Paragraph::new(endpoints).block(ep_block);
        frame.render_widget(ep_para, bottom_cols[1]);
    }

    fn render_kpi(&self, frame: &mut Frame, area: Rect, label: &str, value: f64, color: Color) {
        let block = Block::default().borders(Borders::ALL).title(label);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let value_str = if value >= 1000.0 {
            format!("{:.1}k", value / 1000.0)
        } else {
            format!("{:.1}", value)
        };

        let text = Paragraph::new(value_str)
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        // Center vertically
        if inner.height > 1 {
            let y_offset = inner.height / 2;
            let centered = Rect::new(inner.x, inner.y + y_offset, inner.width, 1);
            frame.render_widget(text, centered);
        } else {
            frame.render_widget(text, inner);
        }
    }

    fn render_sparkline(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        data: &[u64],
        color: Color,
    ) {
        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if data.is_empty() {
            let msg = Paragraph::new("No data yet")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(msg, inner);
            return;
        }

        let sparkline = Sparkline::default()
            .data(data)
            .style(Style::default().fg(color));
        frame.render_widget(sparkline, inner);
    }

    fn render_rates(&self, frame: &mut Frame, area: Rect) {
        let chunks =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

        self.render_metric_detail(frame, chunks[0], &self.request_rate, Color::Blue);
        self.render_metric_detail(frame, chunks[1], &self.error_rate, Color::Red);
    }

    fn render_latency(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(area);

        self.render_metric_detail(frame, chunks[0], &self.p50_latency, Color::Green);
        self.render_metric_detail(frame, chunks[1], &self.p95_latency, Color::Yellow);
        self.render_metric_detail(frame, chunks[2], &self.p99_latency, Color::Red);
    }

    fn render_errors(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([Constraint::Length(8), Constraint::Min(5)]).split(area);

        self.render_metric_detail(frame, chunks[0], &self.error_rate, Color::Red);

        // Status code distribution as bar chart
        if !self.status_distribution.is_empty() {
            let bars: Vec<Bar> = self
                .status_distribution
                .iter()
                .map(|(label, val)| {
                    let color = match label.as_str() {
                        "2xx" => Color::Green,
                        "3xx" => Color::Cyan,
                        "4xx" => Color::Yellow,
                        "5xx" => Color::Red,
                        _ => Color::Gray,
                    };
                    Bar::default()
                        .value(*val)
                        .label(Line::from(label.as_str()))
                        .style(Style::default().fg(color))
                })
                .collect();

            let bar_chart = BarChart::default()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Status Distribution"),
                )
                .data(BarGroup::default().bars(&bars))
                .bar_width(8)
                .bar_gap(2);
            frame.render_widget(bar_chart, chunks[1]);
        } else {
            let msg = Paragraph::new("No status data")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Status Distribution"),
                )
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(msg, chunks[1]);
        }
    }

    fn render_metric_detail(
        &self,
        frame: &mut Frame,
        area: Rect,
        metric: &DashboardMetric,
        color: Color,
    ) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!("{} ({})", metric.name, metric.unit));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let cols = Layout::horizontal([Constraint::Length(20), Constraint::Min(10)]).split(inner);

        // Stats column
        let stats = vec![
            Line::from(vec![
                Span::raw("Current: "),
                Span::styled(
                    format!("{:.2}", metric.current),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("Min:     "),
                Span::styled(
                    format!("{:.2}", metric.min),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(vec![
                Span::raw("Max:     "),
                Span::styled(
                    format!("{:.2}", metric.max),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(vec![
                Span::raw("Avg:     "),
                Span::styled(
                    format!("{:.2}", metric.avg),
                    Style::default().fg(Color::Gray),
                ),
            ]),
        ];
        let stats_para = Paragraph::new(stats);
        frame.render_widget(stats_para, cols[0]);

        // Sparkline
        if !metric.series.is_empty() {
            let sparkline = Sparkline::default()
                .data(&metric.series)
                .style(Style::default().fg(color));
            frame.render_widget(sparkline, cols[1]);
        }
    }
}

impl Component for LiveDashboard {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('L') => {
                self.visible = false;
                Some(Action::Noop)
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.active_tab = match self.active_tab {
                    DashboardTab::Overview => DashboardTab::Errors,
                    DashboardTab::Rates => DashboardTab::Overview,
                    DashboardTab::Latency => DashboardTab::Rates,
                    DashboardTab::Errors => DashboardTab::Latency,
                };
                Some(Action::Noop)
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.active_tab = match self.active_tab {
                    DashboardTab::Overview => DashboardTab::Rates,
                    DashboardTab::Rates => DashboardTab::Latency,
                    DashboardTab::Latency => DashboardTab::Errors,
                    DashboardTab::Errors => DashboardTab::Overview,
                };
                Some(Action::Noop)
            }
            KeyCode::Char('1') => {
                self.active_tab = DashboardTab::Overview;
                Some(Action::Noop)
            }
            KeyCode::Char('2') => {
                self.active_tab = DashboardTab::Rates;
                Some(Action::Noop)
            }
            KeyCode::Char('3') => {
                self.active_tab = DashboardTab::Latency;
                Some(Action::Noop)
            }
            KeyCode::Char('4') => {
                self.active_tab = DashboardTab::Errors;
                Some(Action::Noop)
            }
            _ => None,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        if !self.visible {
            return;
        }

        let overlay = self.overlay_area(area);
        frame.render_widget(Clear, overlay);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Live Dashboard ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(self.theme.bg));
        let inner = block.inner(overlay);
        frame.render_widget(block, overlay);

        let chunks = Layout::vertical([
            Constraint::Length(1), // Tabs
            Constraint::Min(5),    // Content
        ])
        .split(inner);

        // Tab bar
        let tab_idx = match self.active_tab {
            DashboardTab::Overview => 0,
            DashboardTab::Rates => 1,
            DashboardTab::Latency => 2,
            DashboardTab::Errors => 3,
        };
        let tabs = Tabs::new(TABS.iter().map(|t| Line::from(*t)))
            .select(tab_idx)
            .highlight_style(
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            );
        frame.render_widget(tabs, chunks[0]);

        // Content
        match self.active_tab {
            DashboardTab::Overview => self.render_overview(frame, chunks[1]),
            DashboardTab::Rates => self.render_rates(frame, chunks[1]),
            DashboardTab::Latency => self.render_latency(frame, chunks[1]),
            DashboardTab::Errors => self.render_errors(frame, chunks[1]),
        }
    }
}

fn empty_metric(name: &str, unit: &str) -> DashboardMetric {
    DashboardMetric {
        name: name.to_string(),
        unit: unit.to_string(),
        current: 0.0,
        min: 0.0,
        max: 0.0,
        avg: 0.0,
        series: Vec::new(),
    }
}
