//! Incident timeline overlay.
//!
//! A chronological view of incidents, anomalies, and marked events. Users can
//! mark incident boundaries and see correlated changes across log sources.
//!
//! Keybinding: `I` in normal mode.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Severity of an incident event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncidentSeverity {
    Info,
    Warning,
    Critical,
}

/// A single event in the incident timeline.
#[derive(Debug, Clone)]
pub struct IncidentEvent {
    /// When this event occurred (formatted string).
    pub timestamp: String,
    /// Unix timestamp in seconds (for sorting).
    pub unix_ts: i64,
    /// Severity level.
    pub severity: IncidentSeverity,
    /// Short title of the event.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Which source/service this event came from.
    pub source: String,
    /// Related log entries count.
    pub related_count: usize,
    /// Whether this was manually marked or auto-detected.
    pub auto_detected: bool,
}

/// The incident timeline component.
pub struct IncidentTimeline {
    visible: bool,
    theme: Theme,
    events: Vec<IncidentEvent>,
    list_state: ListState,
    detail_visible: bool,
}

impl IncidentTimeline {
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            theme,
            events: Vec::new(),
            list_state: ListState::default(),
            detail_visible: false,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Add an event to the timeline (keeps events sorted by time descending).
    pub fn add_event(&mut self, event: IncidentEvent) {
        self.events.push(event);
        self.events.sort_by(|a, b| b.unix_ts.cmp(&a.unix_ts));
    }

    /// Add an auto-detected anomaly event.
    pub fn add_anomaly(
        &mut self,
        timestamp: String,
        unix_ts: i64,
        description: String,
        source: String,
    ) {
        self.add_event(IncidentEvent {
            timestamp,
            unix_ts,
            severity: IncidentSeverity::Warning,
            title: "Anomaly Detected".to_string(),
            description,
            source,
            related_count: 0,
            auto_detected: true,
        });
    }

    /// Add a manually marked incident.
    pub fn mark_incident(&mut self, title: String) {
        let now = chrono::Utc::now();
        self.add_event(IncidentEvent {
            timestamp: now.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            unix_ts: now.timestamp(),
            severity: IncidentSeverity::Critical,
            title,
            description: "Manually marked incident".to_string(),
            source: "user".to_string(),
            related_count: 0,
            auto_detected: false,
        });
    }

    /// Clear all events.
    pub fn clear(&mut self) {
        self.events.clear();
        self.list_state.select(None);
    }

    fn overlay_area(&self, area: Rect) -> Rect {
        let h = (area.height as f32 * 0.80) as u16;
        let w = (area.width as f32 * 0.85) as u16;
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

    fn selected_event(&self) -> Option<&IncidentEvent> {
        self.list_state.selected().and_then(|i| self.events.get(i))
    }
}

impl Component for IncidentTimeline {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('I') => {
                if self.detail_visible {
                    self.detail_visible = false;
                } else {
                    self.visible = false;
                }
                Some(Action::Noop)
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let len = self.events.len();
                if len == 0 {
                    return Some(Action::Noop);
                }
                let next = match self.list_state.selected() {
                    Some(i) => (i + 1).min(len - 1),
                    None => 0,
                };
                self.list_state.select(Some(next));
                Some(Action::Noop)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let next = match self.list_state.selected() {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.list_state.select(Some(next));
                Some(Action::Noop)
            }
            KeyCode::Enter => {
                self.detail_visible = !self.detail_visible;
                Some(Action::Noop)
            }
            KeyCode::Char('m') => Some(Action::MarkIncident("Manual incident mark".to_string())),
            KeyCode::Char('c') => {
                self.clear();
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
            .title(" Incident Timeline ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(self.theme.bg));
        let inner = block.inner(overlay);
        frame.render_widget(block, overlay);

        if self.events.is_empty() {
            let msg = Paragraph::new(vec![
                Line::from(""),
                Line::from("  No incidents recorded yet."),
                Line::from(""),
                Line::from("  Press 'm' to mark an incident boundary."),
                Line::from("  Anomalies from the analytics engine will appear here automatically."),
            ])
            .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(msg, inner);
            return;
        }

        let chunks = if self.detail_visible {
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(inner)
        } else {
            Layout::horizontal([Constraint::Percentage(100)]).split(inner)
        };

        // Event list
        let items: Vec<ListItem> = self
            .events
            .iter()
            .map(|event| {
                let (icon, color) = match event.severity {
                    IncidentSeverity::Info => ("i", Color::Blue),
                    IncidentSeverity::Warning => ("!", Color::Yellow),
                    IncidentSeverity::Critical => ("X", Color::Red),
                };

                let auto_tag = if event.auto_detected { " [auto]" } else { "" };

                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(
                            format!(" [{icon}] "),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(&event.timestamp, Style::default().fg(Color::DarkGray)),
                        Span::styled(auto_tag, Style::default().fg(Color::DarkGray)),
                    ]),
                    Line::from(vec![
                        Span::raw("     "),
                        Span::styled(
                            &event.title,
                            Style::default()
                                .fg(self.theme.fg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("  [{}]", event.source),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]),
                ])
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::RIGHT))
            .highlight_style(
                Style::default()
                    .bg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            );

        let mut state = self.list_state.clone();
        frame.render_stateful_widget(list, chunks[0], &mut state);

        // Detail panel
        if self.detail_visible && chunks.len() > 1 {
            if let Some(event) = self.selected_event() {
                let (severity_str, severity_color) = match event.severity {
                    IncidentSeverity::Info => ("INFO", Color::Blue),
                    IncidentSeverity::Warning => ("WARNING", Color::Yellow),
                    IncidentSeverity::Critical => ("CRITICAL", Color::Red),
                };

                let detail = vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  Severity: ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            severity_str,
                            Style::default()
                                .fg(severity_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  Time:     ", Style::default().fg(Color::DarkGray)),
                        Span::raw(&event.timestamp),
                    ]),
                    Line::from(vec![
                        Span::styled("  Source:   ", Style::default().fg(Color::DarkGray)),
                        Span::styled(&event.source, Style::default().fg(Color::Cyan)),
                    ]),
                    Line::from(vec![
                        Span::styled("  Related:  ", Style::default().fg(Color::DarkGray)),
                        Span::raw(format!("{} log entries", event.related_count)),
                    ]),
                    Line::from(vec![
                        Span::styled("  Type:     ", Style::default().fg(Color::DarkGray)),
                        Span::raw(if event.auto_detected {
                            "Auto-detected"
                        } else {
                            "Manual"
                        }),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "  Description:",
                        Style::default().fg(Color::DarkGray),
                    )),
                    Line::from(format!("  {}", event.description)),
                ];

                let detail_block = Block::default().borders(Borders::LEFT).title(" Details ");
                let detail_para = Paragraph::new(detail)
                    .block(detail_block)
                    .wrap(Wrap { trim: false });
                frame.render_widget(detail_para, chunks[1]);
            }
        }
    }
}
