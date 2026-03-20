//! Regex playground overlay.
//!
//! An interactive overlay where users write a regular expression and see
//! live matches against the current log buffer. Matched regions are
//! highlighted, and named/numbered capture groups are displayed.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use regex::Regex;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Maximum number of matches to retain (to avoid unbounded memory use).
const MAX_MATCHES: usize = 500;

// ── Data types ───────────────────────────────────────────────────────────

/// A single regex match against a log line.
struct RegexMatch {
    /// Index of the source line in [`RegexPlayground::lines`].
    #[allow(dead_code)]
    entry_idx: usize,
    /// The full log line that matched.
    line: String,
    /// Byte-offset ranges of each match within the line.
    match_ranges: Vec<(usize, usize)>,
    /// Named or numbered capture groups: (group name/index, captured text).
    captures: Vec<(String, String)>,
}

// ── Component ────────────────────────────────────────────────────────────

/// Overlay component for live regex testing against log lines.
pub struct RegexPlayground {
    /// Whether the overlay is visible.
    visible: bool,
    /// Color theme.
    theme: Theme,
    /// Text input for the regex pattern.
    input: Input,
    /// Whether the current regex compiles successfully.
    is_valid: bool,
    /// Compilation error message (empty when valid).
    error_msg: String,
    /// Matched results.
    matches: Vec<RegexMatch>,
    /// Total number of lines being searched.
    total_entries: usize,
    /// Scroll offset for the match list.
    scroll: u16,
    /// Log lines to match against (owned to avoid lifetime issues).
    lines: Vec<String>,
}

impl RegexPlayground {
    /// Create a new regex playground overlay.
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            theme,
            input: Input::default(),
            is_valid: true,
            error_msg: String::new(),
            matches: Vec::new(),
            total_entries: 0,
            scroll: 0,
            lines: Vec::new(),
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
            self.input = Input::default();
            self.is_valid = true;
            self.error_msg.clear();
            self.matches.clear();
        }
    }

    /// Store log lines for matching.
    pub fn set_lines(&mut self, lines: Vec<String>) {
        self.total_entries = lines.len();
        self.lines = lines;
    }

    /// Recompile the regex and scan all lines, populating matches.
    pub fn evaluate(&mut self) {
        self.matches.clear();
        self.scroll = 0;

        let pattern = self.input.value();
        if pattern.is_empty() {
            self.is_valid = true;
            self.error_msg.clear();
            return;
        }

        let re = match Regex::new(pattern) {
            Ok(r) => {
                self.is_valid = true;
                self.error_msg.clear();
                r
            }
            Err(e) => {
                self.is_valid = false;
                self.error_msg = e.to_string();
                return;
            }
        };

        let capture_names: Vec<Option<String>> =
            re.capture_names().map(|n| n.map(String::from)).collect();

        for (idx, line) in self.lines.iter().enumerate() {
            if self.matches.len() >= MAX_MATCHES {
                break;
            }

            let Some(caps) = re.captures(line) else {
                continue;
            };

            let overall = caps.get(0).unwrap();
            let mut match_ranges = vec![(overall.start(), overall.end())];

            // Also collect sub-match ranges for additional groups.
            for i in 1..caps.len() {
                if let Some(m) = caps.get(i) {
                    match_ranges.push((m.start(), m.end()));
                }
            }

            // Collect capture groups.
            let mut captures = Vec::new();
            for (i, name_opt) in capture_names.iter().enumerate().skip(1) {
                if let Some(m) = caps.get(i) {
                    let group_name = name_opt.clone().unwrap_or_else(|| format!("{}", i));
                    captures.push((group_name, m.as_str().to_string()));
                }
            }

            self.matches.push(RegexMatch {
                entry_idx: idx,
                line: line.clone(),
                match_ranges,
                captures,
            });
        }
    }

    // ── Rendering helpers ────────────────────────────────────────────

    /// Build highlighted spans for a matched line.
    ///
    /// Text within match ranges is rendered with the search highlight style;
    /// everything else is rendered in the default foreground.
    fn highlight_line<'a>(&self, line: &'a str, ranges: &[(usize, usize)]) -> Line<'a> {
        let highlight = self.theme.search_highlight();
        let normal = Style::default().fg(self.theme.fg);

        if ranges.is_empty() {
            return Line::from(Span::styled(line.to_string(), normal));
        }

        // Use only the first (overall) match range for highlighting.
        // Byte offsets from the regex engine must land on valid UTF-8 char
        // boundaries — clamp to the nearest valid boundary to avoid panics.
        let (raw_start, raw_end) = ranges[0];
        let mut start = raw_start.min(line.len());
        let mut end = raw_end.min(line.len());
        while start > 0 && !line.is_char_boundary(start) {
            start -= 1;
        }
        while end < line.len() && !line.is_char_boundary(end) {
            end += 1;
        }

        let mut spans = Vec::new();
        if start > 0 {
            spans.push(Span::styled(line[..start].to_string(), normal));
        }
        if start < end {
            spans.push(Span::styled(line[start..end].to_string(), highlight));
        }
        if end < line.len() {
            spans.push(Span::styled(line[end..].to_string(), normal));
        }

        Line::from(spans)
    }
}

// ── Component trait ──────────────────────────────────────────────────────

impl Component for RegexPlayground {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if !self.visible {
            return None;
        }

        match key.code {
            KeyCode::Esc => {
                self.visible = false;
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                // Apply current regex as a search term.
                let term = self.input.value().to_string();
                self.visible = false;
                if term.is_empty() {
                    Some(Action::Noop)
                } else {
                    Some(Action::SearchSubmit(term))
                }
            }
            // Scroll through matches.
            KeyCode::Down
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.scroll = self.scroll.saturating_add(1);
                Some(Action::Noop)
            }
            KeyCode::Up
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.scroll = self.scroll.saturating_sub(1);
                Some(Action::Noop)
            }
            // PageDown / PageUp for larger scrolling.
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(10);
                Some(Action::Noop)
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(10);
                Some(Action::Noop)
            }
            // All other keys go to the text input, then re-evaluate.
            _ => {
                self.input.handle_event(&crossterm::event::Event::Key(key));
                self.evaluate();
                Some(Action::Noop)
            }
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
            .title(" Regex Playground ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        if inner.height < 4 || inner.width < 20 {
            return;
        }

        // Layout: input bar, status line, matches, footer.
        let chunks = Layout::vertical([
            Constraint::Length(1), // Regex input
            Constraint::Length(1), // Status / error
            Constraint::Min(1),    // Match results
            Constraint::Length(1), // Footer
        ])
        .split(inner);

        let input_area = chunks[0];
        let status_area = chunks[1];
        let matches_area = chunks[2];
        let footer_area = chunks[3];

        // ── Input line ──────────────────────────────────────────────
        let validity_indicator = if self.input.value().is_empty() {
            Span::styled(" ? ", Style::default().fg(self.theme.fg_dim))
        } else if self.is_valid {
            Span::styled(
                " \u{2714} ",
                Style::default()
                    .fg(self.theme.info)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                " \u{2718} ",
                Style::default()
                    .fg(self.theme.error)
                    .add_modifier(Modifier::BOLD),
            )
        };

        let input_line = Line::from(vec![
            Span::styled(
                " regex: ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(self.input.value()),
            validity_indicator,
        ]);

        let input_para = Paragraph::new(input_line);
        frame.render_widget(input_para, input_area);

        // Cursor position.
        let cursor_x = input_area.x + 8 + self.input.visual_cursor() as u16;
        let cursor_y = input_area.y;
        frame.set_cursor_position((cursor_x, cursor_y));

        // ── Status line ─────────────────────────────────────────────
        let status_line = if !self.error_msg.is_empty() {
            Line::from(Span::styled(
                format!(
                    " Error: {}",
                    truncate_str(&self.error_msg, inner.width as usize - 10)
                ),
                Style::default().fg(self.theme.error),
            ))
        } else if self.input.value().is_empty() {
            Line::from(Span::styled(
                " Type a regex to search log lines...",
                Style::default().fg(self.theme.fg_dim),
            ))
        } else {
            Line::from(Span::styled(
                format!(
                    " {} matches across {} lines",
                    self.matches.len(),
                    self.total_entries,
                ),
                Style::default().fg(self.theme.fg_dim),
            ))
        };
        frame.render_widget(Paragraph::new(status_line), status_area);

        // ── Match results ───────────────────────────────────────────
        if self.matches.is_empty() {
            let empty_msg = if self.input.value().is_empty() || !self.is_valid {
                ""
            } else {
                " No matches."
            };
            let p = Paragraph::new(Line::from(Span::styled(
                empty_msg,
                Style::default().fg(self.theme.fg_dim),
            )));
            frame.render_widget(p, matches_area);
        } else {
            let mut lines: Vec<Line> = Vec::new();

            for m in &self.matches {
                // Highlighted match line.
                lines.push(self.highlight_line(&m.line, &m.match_ranges));

                // Show captures if any.
                if !m.captures.is_empty() {
                    let capture_spans: Vec<Span> = m
                        .captures
                        .iter()
                        .map(|(name, val)| {
                            Span::styled(
                                format!("  {name}={val}"),
                                Style::default().fg(self.theme.fg_dim),
                            )
                        })
                        .collect();
                    lines.push(Line::from(capture_spans));
                }
            }

            let paragraph = Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0));
            frame.render_widget(paragraph, matches_area);
        }

        // ── Footer ──────────────────────────────────────────────────
        let dim = Style::default().fg(self.theme.fg_dim);
        let footer = Paragraph::new(Line::from(Span::styled(
            " [Enter] apply as search  [Ctrl+\u{2191}/\u{2193}] scroll  [Esc] close",
            dim,
        )));
        frame.render_widget(footer, footer_area);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

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
