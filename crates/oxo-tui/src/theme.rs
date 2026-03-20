//! Color theme and styling for the TUI.
//!
//! Uses ratatui's indexed colors (256-color palette) by default for broad
//! terminal compatibility, including over SSH and inside tmux.
//!
//! ## Custom themes
//!
//! Users can select a preset and optionally override individual colors in
//! their config file via the `[theme]` section:
//!
//! ```toml
//! [theme]
//! preset = "dracula"
//! accent = "#50fa7b"
//! ```

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

/// User-facing theme configuration, deserializable from TOML.
///
/// Every field is optional — `preset` selects a base palette, and individual
/// color fields override specific slots.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ThemeConfig {
    /// Name of a built-in preset: `"default"`, `"solarized_dark"`,
    /// `"dracula"`, `"monokai"`, or `"light"`.
    pub preset: String,
    /// Override: main background color.
    pub bg: Option<String>,
    /// Override: main foreground color.
    pub fg: Option<String>,
    /// Override: dimmed/secondary text color.
    pub fg_dim: Option<String>,
    /// Override: accent color (focused borders, highlights).
    pub accent: Option<String>,
    /// Override: error-level color.
    pub error: Option<String>,
    /// Override: warning-level color.
    pub warn: Option<String>,
    /// Override: info-level color.
    pub info: Option<String>,
    /// Override: debug-level color.
    pub debug: Option<String>,
}

/// Parse a color string into a ratatui [`Color`].
///
/// Supported formats:
/// - Named colors: `"red"`, `"green"`, `"cyan"`, `"dark_gray"`, etc.
/// - Hex RGB: `"#ff5500"` or `"#f50"`
/// - 256-color index: `"color(123)"`
///
/// Returns `None` for unrecognized strings.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();

    // Hex colors: #rgb or #rrggbb
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    // Indexed: color(N)
    if let Some(inner) = s.strip_prefix("color(").and_then(|s| s.strip_suffix(')')) {
        if let Ok(idx) = inner.trim().parse::<u8>() {
            return Some(Color::Indexed(idx));
        }
        return None;
    }

    // Named colors (case-insensitive).
    match s.to_lowercase().replace('-', "_").as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "dark_gray" | "dark_grey" | "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "light_red" | "lightred" => Some(Color::LightRed),
        "light_green" | "lightgreen" => Some(Color::LightGreen),
        "light_yellow" | "lightyellow" => Some(Color::LightYellow),
        "light_blue" | "lightblue" => Some(Color::LightBlue),
        "light_magenta" | "lightmagenta" => Some(Color::LightMagenta),
        "light_cyan" | "lightcyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        "reset" => Some(Color::Reset),
        _ => None,
    }
}

/// Parse a hex color string (without the `#` prefix).
fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim();
    match hex.len() {
        // Short form: #rgb → expand to #rrggbb
        3 => {
            let chars: Vec<char> = hex.chars().collect();
            let r = u8::from_str_radix(&format!("{0}{0}", chars[0]), 16).ok()?;
            let g = u8::from_str_radix(&format!("{0}{0}", chars[1]), 16).ok()?;
            let b = u8::from_str_radix(&format!("{0}{0}", chars[2]), 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

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
    /// Solarized Dark theme.
    pub fn solarized_dark() -> Self {
        Self {
            bg: Color::Rgb(0x00, 0x2b, 0x36),     // base03
            fg: Color::Rgb(0x83, 0x94, 0x96),     // base0
            fg_dim: Color::Rgb(0x58, 0x6e, 0x75), // base01
            accent: Color::Rgb(0x26, 0x8b, 0xd2), // blue
            error: Color::Rgb(0xdc, 0x32, 0x2f),  // red
            warn: Color::Rgb(0xb5, 0x89, 0x00),   // yellow
            info: Color::Rgb(0x85, 0x99, 0x00),   // green
            debug: Color::Rgb(0x2a, 0xa1, 0x98),  // cyan
            border_focused: Color::Rgb(0x26, 0x8b, 0xd2),
            border_unfocused: Color::Rgb(0x58, 0x6e, 0x75),
            status_bg: Color::Rgb(0x07, 0x36, 0x42), // base02
            status_fg: Color::Rgb(0x93, 0xa1, 0xa1), // base1
            search_match: Color::Rgb(0x00, 0x2b, 0x36),
            sparkline: Color::Rgb(0x26, 0x8b, 0xd2),
        }
    }

    /// Dracula theme.
    pub fn dracula() -> Self {
        Self {
            bg: Color::Rgb(0x28, 0x2a, 0x36),     // background
            fg: Color::Rgb(0xf8, 0xf8, 0xf2),     // foreground
            fg_dim: Color::Rgb(0x62, 0x72, 0xa4), // comment
            accent: Color::Rgb(0xbd, 0x93, 0xf9), // purple
            error: Color::Rgb(0xff, 0x55, 0x55),  // red
            warn: Color::Rgb(0xff, 0xb8, 0x6c),   // orange
            info: Color::Rgb(0x50, 0xfa, 0x7b),   // green
            debug: Color::Rgb(0x8b, 0xe9, 0xfd),  // cyan
            border_focused: Color::Rgb(0xbd, 0x93, 0xf9),
            border_unfocused: Color::Rgb(0x62, 0x72, 0xa4),
            status_bg: Color::Rgb(0x44, 0x47, 0x5a), // current line
            status_fg: Color::Rgb(0xf8, 0xf8, 0xf2),
            search_match: Color::Rgb(0x28, 0x2a, 0x36),
            sparkline: Color::Rgb(0xbd, 0x93, 0xf9),
        }
    }

    /// Monokai theme.
    pub fn monokai() -> Self {
        Self {
            bg: Color::Rgb(0x27, 0x28, 0x22),     // background
            fg: Color::Rgb(0xf8, 0xf8, 0xf2),     // foreground
            fg_dim: Color::Rgb(0x75, 0x71, 0x5e), // comment
            accent: Color::Rgb(0xa6, 0xe2, 0x2e), // green
            error: Color::Rgb(0xf9, 0x26, 0x72),  // pink/red
            warn: Color::Rgb(0xe6, 0xdb, 0x74),   // yellow
            info: Color::Rgb(0xa6, 0xe2, 0x2e),   // green
            debug: Color::Rgb(0x66, 0xd9, 0xef),  // blue/cyan
            border_focused: Color::Rgb(0xa6, 0xe2, 0x2e),
            border_unfocused: Color::Rgb(0x75, 0x71, 0x5e),
            status_bg: Color::Rgb(0x3e, 0x3d, 0x32), // line highlight
            status_fg: Color::Rgb(0xf8, 0xf8, 0xf2),
            search_match: Color::Rgb(0x27, 0x28, 0x22),
            sparkline: Color::Rgb(0xa6, 0xe2, 0x2e),
        }
    }

    /// Light theme (dark text on light background).
    pub fn light() -> Self {
        Self {
            bg: Color::Rgb(0xfa, 0xfa, 0xfa),
            fg: Color::Rgb(0x38, 0x38, 0x38),
            fg_dim: Color::Rgb(0xa0, 0xa0, 0xa0),
            accent: Color::Rgb(0x00, 0x5f, 0xaf), // blue
            error: Color::Rgb(0xd7, 0x00, 0x00),  // red
            warn: Color::Rgb(0xaf, 0x87, 0x00),   // dark yellow
            info: Color::Rgb(0x00, 0x87, 0x00),   // green
            debug: Color::Rgb(0x00, 0x5f, 0x87),  // teal
            border_focused: Color::Rgb(0x00, 0x5f, 0xaf),
            border_unfocused: Color::Rgb(0xc0, 0xc0, 0xc0),
            status_bg: Color::Rgb(0xe0, 0xe0, 0xe0),
            status_fg: Color::Rgb(0x38, 0x38, 0x38),
            search_match: Color::Rgb(0xfa, 0xfa, 0xfa),
            sparkline: Color::Rgb(0x00, 0x5f, 0xaf),
        }
    }

    /// Build a theme from a [`ThemeConfig`].
    ///
    /// Starts with the preset palette (falling back to `default` if the preset
    /// name is unrecognized), then applies any per-field color overrides.
    pub fn from_config(config: &ThemeConfig) -> Self {
        let mut theme = match config.preset.to_lowercase().as_str() {
            "solarized_dark" | "solarized-dark" | "solarized" => Self::solarized_dark(),
            "dracula" => Self::dracula(),
            "monokai" => Self::monokai(),
            "light" => Self::light(),
            _ => Self::default(),
        };

        // Apply optional overrides.
        if let Some(c) = config.bg.as_deref().and_then(parse_color) {
            theme.bg = c;
        }
        if let Some(c) = config.fg.as_deref().and_then(parse_color) {
            theme.fg = c;
        }
        if let Some(c) = config.fg_dim.as_deref().and_then(parse_color) {
            theme.fg_dim = c;
        }
        if let Some(c) = config.accent.as_deref().and_then(parse_color) {
            theme.accent = c;
            // Accent also drives focused border and sparkline by default.
            theme.border_focused = c;
            theme.sparkline = c;
        }
        if let Some(c) = config.error.as_deref().and_then(parse_color) {
            theme.error = c;
        }
        if let Some(c) = config.warn.as_deref().and_then(parse_color) {
            theme.warn = c;
        }
        if let Some(c) = config.info.as_deref().and_then(parse_color) {
            theme.info = c;
        }
        if let Some(c) = config.debug.as_deref().and_then(parse_color) {
            theme.debug = c;
        }

        theme
    }

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
    /// Recognized levels: "fatal"/"critical", "error"/"err",
    /// "warn"/"warning", "info", "debug", "trace".
    pub fn log_level_style(&self, level: &str) -> Style {
        match level.to_lowercase().as_str() {
            "fatal" | "critical" => Style::default().fg(self.error).add_modifier(Modifier::BOLD),
            "error" | "err" => Style::default().fg(self.error),
            "warn" | "warning" => Style::default().fg(self.warn),
            "info" => Style::default().fg(self.info),
            "debug" => Style::default().fg(self.fg_dim),
            "trace" => Style::default().fg(self.fg_dim),
            _ => Style::default().fg(self.fg),
        }
    }

    /// Return the foreground color for a log level (used to tint entire lines).
    pub fn log_level_color(&self, level: &str) -> Option<Color> {
        match level.to_lowercase().as_str() {
            "fatal" | "critical" => Some(self.error),
            "error" | "err" => Some(self.error),
            "warn" | "warning" => Some(self.warn),
            "info" => Some(self.info),
            "debug" | "trace" => Some(self.fg_dim),
            _ => None,
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
