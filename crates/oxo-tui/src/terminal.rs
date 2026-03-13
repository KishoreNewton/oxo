//! Terminal initialization and restoration.
//!
//! Sets up the terminal for raw mode and the alternate screen, and ensures
//! proper cleanup on exit (even on panic).

use std::io::{self, stdout};

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// Type alias for our terminal backend.
pub type Tui = Terminal<CrosstermBackend<io::Stdout>>;

/// Initialize the terminal for TUI rendering.
///
/// Enables raw mode, switches to the alternate screen, and installs a
/// panic hook that restores the terminal before printing the panic message.
///
/// # Errors
///
/// Returns an error if terminal setup fails (e.g. not a TTY).
pub fn init() -> io::Result<Tui> {
    // Install a panic hook that restores the terminal first.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore();
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    Ok(terminal)
}

/// Restore the terminal to its original state.
///
/// Disables raw mode and leaves the alternate screen. This is called
/// on normal exit and by the panic hook.
pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}
