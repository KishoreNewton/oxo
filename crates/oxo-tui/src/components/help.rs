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
        let popup_height = 30u16.min(area.height.saturating_sub(4));

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
            Line::from("  w             Toggle line wrap"),
            Line::from("  t             Toggle timestamps"),
            Line::from("  y             Copy selected line"),
            Line::from("  e             Export logs to JSON"),
            Line::from("  ?             Toggle this help"),
            Line::from("  q/Ctrl+c      Quit"),
        ];

        let paragraph = Paragraph::new(help_lines)
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, popup_area);
    }
}
