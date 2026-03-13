//! Color theme and styling for the TUI.
//!
//! Uses ratatui's indexed colors (256-color palette) by default for broad
//! terminal compatibility, including over SSH and inside tmux.

use ratatui::style::{Color, Modifier, Style};

/// The application color theme.
///
/// All UI components reference this struct for their colors, ensuring a
/// consistent look and making it easy to swap palettes in the future.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Background color for the main area.
    pub bg: Color,
    /// Default foreground text color.
    pub fg: Color,
    /// Muted / secondary text color.
    pub fg_dim: Color,
    /// Accent color (focused borders, highlights).
    pub accent: Color,
    /// Color for error messages and error-level logs.
    pub error: Color,
    /// Color for warning-level logs.
    pub warn: Color,
    /// Color for info-level logs.
    pub info: Color,
    /// Color for debug-level logs.
    pub debug: Color,
    /// Border color for focused panels.
    pub border_focused: Color,
    /// Border color for unfocused panels.
    pub border_unfocused: Color,
    /// Status bar background.
    pub status_bg: Color,
    /// Status bar foreground.
    pub status_fg: Color,
    /// Search match highlight color.
    pub search_match: Color,
    /// Sparkline bar color.
    pub sparkline: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::White,
            fg_dim: Color::DarkGray,
            accent: Color::Cyan,
            error: Color::Red,
            warn: Color::Yellow,
            info: Color::Green,
            debug: Color::Blue,
            border_focused: Color::Cyan,
            border_unfocused: Color::DarkGray,
            status_bg: Color::DarkGray,
            status_fg: Color::White,
            search_match: Color::Black,
            sparkline: Color::Cyan,
        }
    }
}

impl Theme {
    /// Base style (default fg on default bg).
    pub fn base(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    /// Style for dimmed / secondary text.
    pub fn dimmed(&self) -> Style {
        Style::default().fg(self.fg_dim)
    }

    /// Style for the status bar.
    pub fn status_bar(&self) -> Style {
        Style::default().fg(self.status_fg).bg(self.status_bg)
    }

    /// Style for a focused panel border.
    pub fn border_focused(&self) -> Style {
        Style::default().fg(self.border_focused)
    }

    /// Style for an unfocused panel border.
    pub fn border_unfocused(&self) -> Style {
        Style::default().fg(self.border_unfocused)
    }

    /// Style for a log line based on its log level string.
    ///
    /// Recognized levels: "error", "warn"/"warning", "info", "debug", "trace".
    pub fn log_level_style(&self, level: &str) -> Style {
        match level.to_lowercase().as_str() {
            "error" | "err" | "fatal" | "critical" => {
                Style::default().fg(self.error).add_modifier(Modifier::BOLD)
            }
            "warn" | "warning" => Style::default().fg(self.warn),
            "info" => Style::default().fg(self.info),
            "debug" => Style::default().fg(self.debug),
            "trace" => Style::default().fg(self.fg_dim),
            _ => Style::default().fg(self.fg),
        }
    }

    /// Style for search match highlighting.
    pub fn search_highlight(&self) -> Style {
        Style::default()
            .fg(self.search_match)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }
}
