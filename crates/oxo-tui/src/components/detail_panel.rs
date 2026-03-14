//! Log detail/inspect panel component.
//!
//! Shows the full content of a selected log entry: all labels, the complete
//! log line (unwrapped), and the raw JSON response (if available). Renders
//! as a right-side split or overlay.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use oxo_core::LogEntry;
use oxo_core::structured::StructuredData;

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
        // Split long lines for readability (char-boundary-aware).
        let chunk_size = panel_width.saturating_sub(4) as usize;
        if chunk_size > 0 {
            let msg = &entry.line;
            let mut start = 0;
            while start < msg.len() {
                let mut end = (start + chunk_size).min(msg.len());
                // Walk back to a char boundary if we landed in the middle of one.
                while end < msg.len() && !msg.is_char_boundary(end) {
                    end -= 1;
                }
                lines.push(Line::from(format!("  {}", &msg[start..end])));
                start = end;
            }
        }

        // Structured Fields (if the log line looks structured).
        if let Some(structured) = StructuredData::parse(&entry.line) {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Structured Fields",
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            lines.push(Line::from(""));

            for (key, value) in structured.fields() {
                // For JSON, values that are themselves objects/arrays are
                // rendered over multiple indented lines for readability.
                if value.starts_with('{') || value.starts_with('[') {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {key}: "),
                        Style::default().fg(self.theme.accent),
                    )]));
                    // Pretty-print and syntax-color the nested JSON if possible.
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&value) {
                        if let Ok(pretty) = serde_json::to_string_pretty(&v) {
                            for nested_line in pretty.lines() {
                                lines.push(Line::from(colorize_json_line(nested_line, "    ")));
                            }
                        } else {
                            lines.push(Line::from(format!("    {value}")));
                        }
                    } else {
                        for nested_line in value.lines() {
                            lines.push(Line::from(format!("    {nested_line}")));
                        }
                    }
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {key}: "), Style::default().fg(self.theme.accent)),
                        Span::raw(value),
                    ]));
                }
            }
        }

        // Raw JSON (if available) — pretty-printed with syntax coloring.
        if let Some(ref raw) = entry.raw {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Raw JSON",
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            lines.push(Line::from(""));
            if let Ok(pretty) = serde_json::to_string_pretty(raw) {
                for json_line in pretty.lines() {
                    lines.push(Line::from(colorize_json_line(json_line, "  ")));
                }
            } else {
                // Fallback: show the raw value unformatted.
                let fallback = raw.to_string();
                for json_line in fallback.lines() {
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

/// Syntax-color a single line of pretty-printed JSON.
///
/// This operates on the output of `serde_json::to_string_pretty()`, which
/// produces one token per line in a predictable format. The function
/// returns a vector of [`Span`]s with colors:
///
/// - Keys → Cyan
/// - String values → Green
/// - Numbers → Yellow
/// - Booleans / null → Magenta
/// - Structural characters (`{`, `}`, `[`, `]`, `,`, `:`) → default
fn colorize_json_line<'a>(line: &str, indent_prefix: &str) -> Vec<Span<'a>> {
    let mut spans: Vec<Span<'a>> = Vec::new();

    // Add the caller-requested indentation prefix.
    if !indent_prefix.is_empty() {
        spans.push(Span::raw(indent_prefix.to_string()));
    }

    let trimmed = line.trim_start();
    let leading_ws = &line[..line.len() - trimmed.len()];
    if !leading_ws.is_empty() {
        spans.push(Span::raw(leading_ws.to_string()));
    }

    if trimmed.is_empty() {
        return spans;
    }

    // Walk through the trimmed content character by character, splitting into
    // tokens. Pretty-printed JSON lines follow a handful of patterns:
    //   "key": value,       (object entry)
    //   "key": "strval",    (object entry with string value)
    //   "string value",     (array element)
    //   123,                (number)
    //   true / false / null
    //   { } [ ] , :
    let bytes = trimmed.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    while pos < len {
        match bytes[pos] {
            // Quoted string — could be a key or a value.
            b'"' => {
                // Find the closing quote (handle escaped quotes).
                let start = pos;
                pos += 1;
                while pos < len {
                    if bytes[pos] == b'\\' {
                        pos += 2; // skip escaped char
                        continue;
                    }
                    if bytes[pos] == b'"' {
                        pos += 1;
                        break;
                    }
                    pos += 1;
                }
                let quoted = &trimmed[start..pos];

                // Peek ahead: if the next non-space character is ':', this is a key.
                let mut peek = pos;
                while peek < len && bytes[peek] == b' ' {
                    peek += 1;
                }
                if peek < len && bytes[peek] == b':' {
                    // JSON key.
                    spans.push(Span::styled(
                        quoted.to_string(),
                        Style::default().fg(Color::Cyan),
                    ));
                } else {
                    // JSON string value.
                    spans.push(Span::styled(
                        quoted.to_string(),
                        Style::default().fg(Color::Green),
                    ));
                }
            }
            // Structural characters.
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                // Collect consecutive structural / whitespace chars.
                let start = pos;
                while pos < len
                    && matches!(bytes[pos], b'{' | b'}' | b'[' | b']' | b',' | b':' | b' ')
                {
                    pos += 1;
                }
                spans.push(Span::raw(trimmed[start..pos].to_string()));
            }
            // Whitespace.
            b' ' => {
                let start = pos;
                while pos < len && bytes[pos] == b' ' {
                    pos += 1;
                }
                spans.push(Span::raw(trimmed[start..pos].to_string()));
            }
            // Numbers, booleans, null.
            _ => {
                let start = pos;
                while pos < len && !matches!(bytes[pos], b',' | b'}' | b']' | b' ' | b':' | b'"') {
                    pos += 1;
                }
                let token = &trimmed[start..pos];
                let color = match token {
                    "true" | "false" | "null" => Color::Magenta,
                    _ => {
                        // Looks like a number if it starts with a digit or '-'.
                        if token.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
                            Color::Yellow
                        } else {
                            Color::Reset
                        }
                    }
                };
                spans.push(Span::styled(token.to_string(), Style::default().fg(color)));
            }
        }
    }

    spans
}
