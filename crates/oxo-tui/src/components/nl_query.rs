//! Natural language query overlay.
//!
//! Translates plain English descriptions into LogQL/KQL queries using
//! pattern matching and heuristics (no LLM dependency).
//!
//! Examples:
//!   "errors in the last hour" → `{} |= "error"`
//!   "show me 500 errors from api service" → `{job="api"} |= "500"`
//!   "slow requests over 2 seconds" → `{} | json | duration > 2000`
//!
//! Keybinding: Ctrl+L in normal mode.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Natural language query component.
pub struct NlQuery {
    visible: bool,
    theme: Theme,
    input: Input,
    generated_query: Option<String>,
    explanation: Vec<String>,
    available_labels: Vec<String>,
    history: Vec<(String, String)>, // (nl_query, generated_query)
}

impl NlQuery {
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            theme,
            input: Input::default(),
            generated_query: None,
            explanation: Vec::new(),
            available_labels: Vec::new(),
            history: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.input.reset();
            self.generated_query = None;
            self.explanation.clear();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Set the available label names for smarter query generation.
    pub fn set_labels(&mut self, labels: Vec<String>) {
        self.available_labels = labels;
    }

    /// Translate a natural language query into a structured query.
    fn translate(&mut self, input: &str) {
        let input_lower = input.to_lowercase();
        let mut selector_parts: Vec<String> = Vec::new();
        let mut line_filters: Vec<String> = Vec::new();
        let mut parse_stages: Vec<String> = Vec::new();
        let mut label_filters: Vec<String> = Vec::new();
        let mut explanations: Vec<String> = Vec::new();

        // Detect service/job mentions
        let service_keywords = ["from", "in", "service", "app", "job", "container"];
        for kw in &service_keywords {
            if let Some(pos) = input_lower.find(kw) {
                let rest = &input[pos + kw.len()..].trim_start();
                if let Some(word) = rest.split_whitespace().next() {
                    let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
                    if !cleaned.is_empty() && cleaned.len() > 1 {
                        // Check if this matches a known label
                        let label_key = if self.available_labels.contains(&"job".to_string()) {
                            "job"
                        } else if self.available_labels.contains(&"app".to_string()) {
                            "app"
                        } else if self.available_labels.contains(&"service".to_string()) {
                            "service"
                        } else {
                            "job"
                        };
                        selector_parts.push(format!("{label_key}=\"{cleaned}\""));
                        explanations.push(format!("Filter to {label_key}=\"{cleaned}\""));
                        break;
                    }
                }
            }
        }

        // Detect log level mentions
        let level_map = [
            ("error", "error"), ("errors", "error"),
            ("warning", "warn"), ("warnings", "warn"), ("warn", "warn"),
            ("info", "info"), ("debug", "debug"),
            ("fatal", "fatal"), ("critical", "fatal"), ("panic", "fatal"),
        ];
        for (word, level) in &level_map {
            if input_lower.contains(word) {
                // Could be a label filter or line filter depending on context
                if self.available_labels.contains(&"level".to_string()) {
                    label_filters.push(format!("level=\"{level}\""));
                    explanations.push(format!("Filter level={level}"));
                } else {
                    line_filters.push(format!("|= \"{level}\""));
                    explanations.push(format!("Match lines containing \"{level}\""));
                }
                break;
            }
        }

        // Detect HTTP status codes
        let status_codes = [
            ("500", "500"), ("5xx", "5"), ("5XX", "5"),
            ("404", "404"), ("4xx", "4"), ("4XX", "4"),
            ("503", "503"), ("502", "502"), ("200", "200"),
        ];
        for (pattern, code) in &status_codes {
            if input_lower.contains(&pattern.to_lowercase()) {
                if pattern.contains("xx") || pattern.contains("XX") {
                    line_filters.push(format!("|~ \"{code}[0-9]{{2}}\""));
                    explanations.push(format!("Match {pattern} status codes"));
                } else {
                    line_filters.push(format!("|= \"{code}\""));
                    explanations.push(format!("Match status code {code}"));
                }
                break;
            }
        }

        // Detect "slow" / "latency" / "timeout" keywords
        if input_lower.contains("slow") || input_lower.contains("latency") || input_lower.contains("timeout") {
            parse_stages.push("json".to_string());

            // Try to extract a threshold number
            let threshold = extract_number(&input_lower).unwrap_or(1000.0);
            let unit = if input_lower.contains("second") { threshold * 1000.0 } else { threshold };

            label_filters.push(format!("duration>{}", unit as u64));
            explanations.push(format!("Parse JSON, filter duration > {}ms", unit as u64));
        }

        // Detect "json" or "structured"
        if input_lower.contains("json") || input_lower.contains("structured") || input_lower.contains("parse") {
            if !parse_stages.contains(&"json".to_string()) {
                parse_stages.push("json".to_string());
                explanations.push("Parse JSON fields".to_string());
            }
        }

        // Detect specific text search patterns
        let search_patterns = [
            ("containing", true), ("with", true),
            ("matching", true), ("like", true),
            ("about", true), ("for", true),
        ];
        for (kw, _) in &search_patterns {
            if let Some(pos) = input_lower.find(kw) {
                let rest = &input[pos + kw.len()..].trim_start();
                // Extract quoted text or the next word(s)
                let search_term = if rest.starts_with('"') {
                    rest.trim_matches('"').split('"').next().unwrap_or("").to_string()
                } else {
                    // Take up to next keyword or end
                    rest.split_whitespace()
                        .take_while(|w| !["from", "in", "last", "since", "between"].contains(&w.to_lowercase().as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                };

                if !search_term.is_empty() && search_term.len() > 1 {
                    line_filters.push(format!("|= \"{}\"", search_term));
                    explanations.push(format!("Match lines containing \"{search_term}\""));
                    break;
                }
            }
        }

        // Detect general text to search (if no other filters matched well)
        if line_filters.is_empty() && label_filters.is_empty() && parse_stages.is_empty() {
            // Use the whole input as a search term, minus common filler words
            let filler = ["show", "me", "the", "all", "find", "get", "logs", "log", "please",
                          "can", "you", "i", "want", "to", "see", "display", "list"];
            let meaningful: Vec<&str> = input_lower
                .split_whitespace()
                .filter(|w| !filler.contains(w))
                .collect();

            if !meaningful.is_empty() {
                let term = meaningful.join(" ");
                line_filters.push(format!("|= \"{}\"", term));
                explanations.push(format!("Search for \"{term}\""));
            }
        }

        // Build the final query
        let selector = if selector_parts.is_empty() {
            "{}".to_string()
        } else {
            format!("{{{}}}", selector_parts.join(", "))
        };

        let mut query = selector;
        for stage in &parse_stages {
            query.push_str(&format!(" | {stage}"));
        }
        for filter in &line_filters {
            query.push_str(&format!(" {filter}"));
        }
        for lf in &label_filters {
            query.push_str(&format!(" | {lf}"));
        }

        // If query is just "{}", at least show something useful
        if query == "{}" {
            query = "{} ".to_string();
            explanations.push("No specific filters detected — showing all logs".to_string());
        }

        self.generated_query = Some(query.clone());
        self.explanation = explanations;
        self.history.push((input.to_string(), query));
    }

    fn overlay_area(&self, area: Rect) -> Rect {
        let h = 20u16.min(area.height - 4);
        let w = 80u16.min(area.width - 4);
        let vertical = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(h),
            Constraint::Fill(2),
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
}

impl Component for NlQuery {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc => {
                self.visible = false;
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                let input_text = self.input.value().to_string();
                if input_text.is_empty() {
                    return Some(Action::Noop);
                }

                if self.generated_query.is_none() {
                    // First Enter: translate NL to query
                    self.translate(&input_text);
                    Some(Action::Noop)
                } else {
                    // Second Enter: submit the generated query
                    let query = self.generated_query.clone().unwrap_or_default();
                    self.visible = false;
                    Some(Action::SubmitQuery(query))
                }
            }
            KeyCode::Tab => {
                // If we have a generated query, allow editing it directly
                if let Some(ref q) = self.generated_query {
                    self.input = Input::new(q.clone());
                    self.generated_query = None;
                    self.explanation.clear();
                }
                Some(Action::Noop)
            }
            _ => {
                // Reset generated query when input changes
                if self.generated_query.is_some() {
                    self.generated_query = None;
                    self.explanation.clear();
                }
                self.input.handle_event(&crossterm::event::Event::Key(key));
                Some(Action::Noop)
            }
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
            .title(" Natural Language Query ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(self.theme.bg));
        let inner = block.inner(overlay);
        frame.render_widget(block, overlay);

        let chunks = Layout::vertical([
            Constraint::Length(1), // Help text
            Constraint::Length(1), // Spacing
            Constraint::Length(3), // Input box
            Constraint::Length(1), // Spacing
            Constraint::Min(3),   // Generated query + explanation
        ])
        .split(inner);

        // Help text
        let help = Line::from(vec![
            Span::styled(
                " Describe what you're looking for in plain English ",
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        frame.render_widget(Paragraph::new(help).alignment(Alignment::Center), chunks[0]);

        // Input box
        let input_width = chunks[2].width.saturating_sub(2) as usize;
        let scroll = self.input.visual_scroll(input_width);
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title("Query")
            .border_style(Style::default().fg(self.theme.accent));
        let input_para = Paragraph::new(self.input.value())
            .block(input_block)
            .scroll((0, scroll as u16));
        frame.render_widget(input_para, chunks[2]);

        // Set cursor position
        let cursor_x = chunks[2].x + 1 + (self.input.visual_cursor().saturating_sub(scroll) as u16);
        let cursor_y = chunks[2].y + 1;
        frame.set_cursor_position((cursor_x, cursor_y));

        // Generated query + explanation
        if let Some(ref query) = self.generated_query {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(" Generated: ", Style::default().fg(Color::DarkGray)),
                ]),
                Line::from(vec![
                    Span::styled(
                        format!("  {query}"),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
            ];

            for exp in &self.explanation {
                lines.push(Line::from(vec![
                    Span::styled("  → ", Style::default().fg(Color::DarkGray)),
                    Span::styled(exp, Style::default().fg(Color::Cyan)),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    " Press Enter to execute  |  Tab to edit  |  Esc to cancel",
                    Style::default().fg(Color::DarkGray),
                ),
            ]));

            let explanation_para = Paragraph::new(lines).wrap(Wrap { trim: false });
            frame.render_widget(explanation_para, chunks[4]);
        } else {
            // Show examples
            let examples = vec![
                Line::from(""),
                Line::from(Span::styled("  Examples:", Style::default().fg(Color::DarkGray))),
                Line::from(vec![
                    Span::styled("  → ", Style::default().fg(Color::DarkGray)),
                    Span::styled("\"errors from api service in the last hour\"", Style::default().fg(Color::Cyan)),
                ]),
                Line::from(vec![
                    Span::styled("  → ", Style::default().fg(Color::DarkGray)),
                    Span::styled("\"slow requests over 2 seconds\"", Style::default().fg(Color::Cyan)),
                ]),
                Line::from(vec![
                    Span::styled("  → ", Style::default().fg(Color::DarkGray)),
                    Span::styled("\"500 errors from payment service\"", Style::default().fg(Color::Cyan)),
                ]),
                Line::from(vec![
                    Span::styled("  → ", Style::default().fg(Color::DarkGray)),
                    Span::styled("\"show me all warnings containing timeout\"", Style::default().fg(Color::Cyan)),
                ]),
            ];
            frame.render_widget(Paragraph::new(examples), chunks[4]);
        }
    }
}

/// Extract the first number found in a string.
fn extract_number(s: &str) -> Option<f64> {
    let mut num_str = String::new();
    let mut found_digit = false;

    for c in s.chars() {
        if c.is_ascii_digit() || (c == '.' && found_digit) {
            num_str.push(c);
            found_digit = true;
        } else if found_digit {
            break;
        }
    }

    num_str.parse().ok()
}
