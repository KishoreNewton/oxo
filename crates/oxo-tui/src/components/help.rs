//! Help overlay component.
//!
//! Displays a centered popup with keyboard shortcuts and usage instructions.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::action::Action;
use crate::components::Component;
use crate::theme::Theme;

/// Help overlay component.
pub struct HelpOverlay {
    visible: bool,
    theme: Theme,
}

impl HelpOverlay {
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            theme,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}

impl Component for HelpOverlay {
    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        if matches!(action, Action::ToggleHelp) {
            self.toggle();
        }
        None
    }

    fn render(&self, frame: &mut Frame, area: Rect, _focused: bool) {
        if !self.visible {
            return;
        }

        let popup_width = 55u16.min(area.width.saturating_sub(4));
        let popup_height = 50u16.min(area.height.saturating_sub(4));

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Help — oxo ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let bold = Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

        let help_lines = vec![
            Line::from(Span::styled("Navigation", bold)),
            Line::from(""),
            Line::from("  j/↓           Scroll down"),
            Line::from("  k/↑           Scroll up"),
            Line::from("  g/Home        Jump to top"),
            Line::from("  G/End         Jump to bottom (tail)"),
            Line::from("  Ctrl+d/PgDn   Page down"),
            Line::from("  Ctrl+u/PgUp   Page up"),
            Line::from("  Tab/Shift+Tab Cycle focus"),
            Line::from("  Space         Toggle line selection"),
            Line::from("  1-9           Switch to tab N"),
            Line::from("  Ctrl+t        New tab"),
            Line::from("  Ctrl+w        Close tab"),
            Line::from(""),
            Line::from(Span::styled("Search & Query", bold)),
            Line::from(""),
            Line::from("  /             Search in logs"),
            Line::from("  n/N           Next/prev search match"),
            Line::from("  :             Enter query mode"),
            Line::from(""),
            Line::from(Span::styled("Actions", bold)),
            Line::from(""),
            Line::from("  Enter         Inspect selected log"),
            Line::from("  f             Toggle filter panel"),
            Line::from("  T             Time range picker"),
            Line::from("  w             Toggle line wrap"),
            Line::from("  t             Toggle timestamps"),
            Line::from("  y             Copy selected line"),
            Line::from("  e             Export logs (JSON)"),
            Line::from("  b             Switch source/backend"),
            Line::from("  s             Log statistics"),
            Line::from("  x             Expand/collapse multi-line"),
            Line::from("  C             Toggle search context"),
            Line::from("  S             Save current query"),
            Line::from(""),
            Line::from(Span::styled("Overlays & Modes", bold)),
            Line::from(""),
            Line::from("  a             Alert history"),
            Line::from("  A             Mute/unmute alerts"),
            Line::from("  i             Analytics dashboard"),
            Line::from("  c             Column/table mode"),
            Line::from("  D             Smart dedup"),
            Line::from("  m             Toggle bookmark"),
            Line::from("  '             Jump to next bookmark"),
            Line::from("  W             Trace waterfall"),
            Line::from("  R             Regex playground"),
            Line::from("  d             Live diff mode"),
            Line::from("  H             Health dashboard"),
            Line::from("  V             Saved views"),
            Line::from("  L             Live metrics dashboard"),
            Line::from("  I             Incident timeline"),
            Line::from("  Ctrl+l        Natural language query"),
            Line::from("  ?             Toggle this help"),
            Line::from("  q/Ctrl+c      Quit"),
        ];

        let paragraph = Paragraph::new(help_lines)
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, popup_area);
    }
}
