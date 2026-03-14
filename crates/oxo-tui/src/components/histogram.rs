//! Time-bucketed histogram component.
//!
//! Replaces the basic sparkline with a proper histogram that shows log volume
//! over time with error/warning coloring per bar. Each bar represents one tick
//! interval and is colored based on the error/warning ratio within that bucket.
//!
//! ## Rate threshold
//!
//! An optional rate threshold line can be displayed on the histogram. When a
//! bucket's total exceeds the threshold it is rendered in the error color,
//! providing a quick visual indicator of traffic spikes. The threshold can be
//! set manually or calculated automatically as `mean + 2 * stddev`.

use std::collections::VecDeque;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols;
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Borders};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Maximum number of time buckets kept in the histogram.
const MAX_BUCKETS: usize = 60;

/// A single time bucket accumulating log entry counts by severity.
#[derive(Debug, Clone, Default)]
pub struct HistogramBucket {
    /// Total entries in this bucket.
    pub total: u64,
    /// Number of error- or fatal-level entries.
    pub errors: u64,
    /// Number of warn-level entries.
    pub warnings: u64,
}

/// Time-bucketed histogram showing log volume with error coloring.
pub struct Histogram {
    /// Rolling window of completed time buckets.
    buckets: VecDeque<HistogramBucket>,
    /// The bucket currently accumulating entries (not yet pushed).
    current_bucket: HistogramBucket,
    /// Color theme.
    theme: Theme,
    /// Optional manual rate threshold (entries per tick).
    threshold: Option<f64>,
    /// When true, threshold is automatically computed from recent data
    /// (mean + 2 * standard deviation).
    auto_threshold: bool,
}

impl Histogram {
    /// Create a new histogram with the given theme.
    pub fn new(theme: Theme) -> Self {
        let mut buckets = VecDeque::with_capacity(MAX_BUCKETS);
        buckets.resize(MAX_BUCKETS, HistogramBucket::default());

        Self {
            buckets,
            current_bucket: HistogramBucket::default(),
            theme,
            threshold: None,
            auto_threshold: false,
        }
    }

    /// Set a manual rate threshold. Pass `None` to clear it.
    pub fn set_threshold(&mut self, threshold: Option<f64>) {
        self.threshold = threshold;
        if threshold.is_some() {
            self.auto_threshold = false;
        }
    }

    /// Toggle automatic threshold calculation on or off.
    ///
    /// When enabled, the threshold is computed each frame as the mean of
    /// recent bucket totals plus two standard deviations.
    pub fn toggle_auto_threshold(&mut self) {
        self.auto_threshold = !self.auto_threshold;
        if self.auto_threshold {
            // Clear any manual threshold — auto takes precedence.
            self.threshold = None;
        }
    }

    /// Compute the effective threshold value (manual or auto).
    fn effective_threshold(&self) -> Option<f64> {
        if let Some(t) = self.threshold {
            return Some(t);
        }
        if self.auto_threshold {
            return self.compute_auto_threshold();
        }
        None
    }

    /// Compute auto-threshold as mean + 2 * stddev of bucket totals.
    fn compute_auto_threshold(&self) -> Option<f64> {
        let values: Vec<f64> = self
            .buckets
            .iter()
            .map(|b| b.total as f64)
            .filter(|v| *v > 0.0)
            .collect();

        if values.len() < 3 {
            return None;
        }

        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        let stddev = variance.sqrt();
        Some(mean + 2.0 * stddev)
    }

    /// Record a single log entry into the current bucket.
    ///
    /// The `level` is matched case-insensitively against common log level
    /// strings to classify the entry as error, warning, or normal.
    pub fn record_entry(&mut self, level: Option<&str>) {
        self.current_bucket.total += 1;

        if let Some(lvl) = level {
            match lvl.to_lowercase().as_str() {
                "error" | "err" | "fatal" | "critical" => {
                    self.current_bucket.errors += 1;
                }
                "warn" | "warning" => {
                    self.current_bucket.warnings += 1;
                }
                _ => {}
            }
        }
    }

    /// Advance to the next time bucket.
    ///
    /// Pushes the current bucket into the rolling window and starts a fresh
    /// one. Called on each [`Action::Tick`].
    pub fn tick(&mut self) {
        if self.buckets.len() >= MAX_BUCKETS {
            self.buckets.pop_front();
        }
        let completed = std::mem::take(&mut self.current_bucket);
        self.buckets.push_back(completed);
    }

    /// Total entries in the current (incomplete) bucket.
    pub fn total_rate(&self) -> u64 {
        self.current_bucket.total
    }

    /// Error percentage for the current (incomplete) bucket.
    pub fn error_rate(&self) -> f64 {
        if self.current_bucket.total == 0 {
            0.0
        } else {
            self.current_bucket.errors as f64 / self.current_bucket.total as f64 * 100.0
        }
    }

    /// Determine the bar color for a bucket based on its error/warning ratio
    /// and threshold.
    fn bucket_color(&self, bucket: &HistogramBucket, threshold: Option<f64>) -> Color {
        // If the bucket exceeds the threshold, always use the error color.
        if let Some(t) = threshold {
            if bucket.total as f64 > t && bucket.total > 0 {
                return self.theme.error;
            }
        }

        if bucket.total == 0 {
            return self.theme.info;
        }
        let error_pct = bucket.errors as f64 / bucket.total as f64;
        let warn_pct = bucket.warnings as f64 / bucket.total as f64;

        if error_pct > 0.30 {
            self.theme.error
        } else if warn_pct > 0.30 {
            self.theme.warn
        } else {
            self.theme.info
        }
    }
}

impl Component for Histogram {
    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        if matches!(action, Action::Tick) {
            self.tick();
        }
        None
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        let error_rate = if self.current_bucket.total == 0 {
            0.0
        } else {
            self.current_bucket.errors as f64 / self.current_bucket.total as f64 * 100.0
        };

        let threshold = self.effective_threshold();

        let title = if let Some(t) = threshold {
            format!(
                " Log Volume (error rate: {:.1}% | threshold: {:.0}) ",
                error_rate, t
            )
        } else {
            format!(" Log Volume (error rate: {:.1}%) ", error_rate)
        };

        let block = Block::default().title(title).borders(Borders::TOP);

        // Build individually-colored bars for each bucket.
        let bars: Vec<Bar> = self
            .buckets
            .iter()
            .map(|bucket| {
                let color = self.bucket_color(bucket, threshold);
                Bar::default()
                    .value(bucket.total)
                    .style(Style::default().fg(color))
            })
            .collect();

        let group = BarGroup::default().bars(&bars);

        let chart = BarChart::default()
            .block(block)
            .data(group)
            .bar_width(1)
            .bar_gap(0);

        frame.render_widget(chart, area);

        // Draw a threshold line overlay if threshold is set and the chart
        // area is tall enough.
        if let Some(t) = threshold {
            self.render_threshold_line(frame, area, t);
        }
    }
}

impl Histogram {
    /// Render a horizontal threshold line across the chart area.
    fn render_threshold_line(&self, frame: &mut Frame, area: Rect, threshold: f64) {
        // The bar chart has a TOP border, so the drawable area starts one row
        // below the area's top.
        let chart_top = area.y + 1;
        let chart_height = area.height.saturating_sub(1);
        if chart_height == 0 {
            return;
        }

        // Determine the max bucket value to scale the threshold position.
        let max_val = self
            .buckets
            .iter()
            .map(|b| b.total)
            .max()
            .unwrap_or(0)
            .max(1) as f64;

        // If threshold is above max, the line would be above the chart.
        if threshold > max_val {
            return;
        }

        // Y position: higher value = higher on screen (lower y).
        let ratio = threshold / max_val;
        let y = chart_top + chart_height - (ratio * chart_height as f64).round() as u16;
        let y = y.clamp(chart_top, chart_top + chart_height - 1);

        let line_style = Style::default().fg(self.theme.warn);
        let buf = frame.buffer_mut();
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_symbol(symbols::line::HORIZONTAL)
                    .set_style(line_style);
            }
        }
    }
}
