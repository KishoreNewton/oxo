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
    /// Whether the help overlay is visible.
    visible: bool,
    /// Color theme.
    theme: Theme,
}

impl HelpOverlay {
    /// Create a new help overlay (hidden by default).
    pub fn new(theme: Theme) -> Self {
        Self {
            visible: false,
            theme,
        }
    }

    /// Whether the overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Toggle visibility.
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

        // Center a popup in the terminal.
        let popup_width = 50u16.min(area.width.saturating_sub(4));
        let popup_height = 22u16.min(area.height.saturating_sub(4));

        let vertical = Layout::vertical([Constraint::Length(popup_height)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Length(popup_width)]).flex(Flex::Center);
        let [vert_area] = vertical.areas(area);
        let [popup_area] = horizontal.areas(vert_area);

        // Clear the area behind the popup.
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Help — oxo ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let help_lines = vec![
            Line::from(Span::styled(
                "Navigation",
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )),
            Line::from(""),
            Line::from("  j/↓         Scroll down"),
            Line::from("  k/↑         Scroll up"),
            Line::from("  g/Home      Jump to top"),
            Line::from("  G/End       Jump to bottom"),
            Line::from("  Ctrl+d/PgDn Page down"),
            Line::from("  Ctrl+u/PgUp Page up"),
            Line::from("  Tab         Cycle focus"),
            Line::from(""),
            Line::from(Span::styled(
                "Actions",
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )),
            Line::from(""),
            Line::from("  / or :      Enter query mode"),
            Line::from("  f           Toggle filter panel"),
            Line::from("  w           Toggle line wrap"),
            Line::from("  t           Toggle timestamps"),
            Line::from("  y           Copy current line"),
            Line::from("  ?           Toggle this help"),
            Line::from("  q/Ctrl+c    Quit"),
        ];

        let paragraph = Paragraph::new(help_lines)
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, popup_area);
    }
}
