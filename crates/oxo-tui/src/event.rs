//! Terminal event handling.
//!
//! Bridges crossterm's event stream into the application's async event loop.
//! Produces terminal events (key press, mouse, resize) that the App
//! translates into [`Action`]s.

use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEvent, MouseEvent};
use futures_util::StreamExt;

/// Terminal events that the app loop handles.
#[derive(Debug)]
pub enum TerminalEvent {
    /// A key was pressed.
    Key(KeyEvent),
    /// A mouse event occurred.
    Mouse(MouseEvent),
    /// The terminal was resized.
    Resize(u16, u16),
    /// A periodic tick fired (for sparkline updates and other time-based work).
    Tick,
}

/// Event reader that merges crossterm events with periodic ticks.
pub struct EventReader {
    /// Crossterm's async event stream.
    event_stream: EventStream,
    /// Tick interval for periodic updates.
    tick_interval: tokio::time::Interval,
}

impl EventReader {
    /// Create a new event reader.
    ///
    /// # Arguments
    ///
    /// * `tick_rate` — How often to emit [`TerminalEvent::Tick`] events.
    pub fn new(tick_rate: Duration) -> Self {
        Self {
            event_stream: EventStream::new(),
            tick_interval: tokio::time::interval(tick_rate),
        }
    }

    /// Wait for the next event.
    ///
    /// Returns `None` if the event stream is exhausted (terminal closed).
    pub async fn next(&mut self) -> Option<TerminalEvent> {
        tokio::select! {
            // Terminal input events.
            maybe_event = self.event_stream.next() => {
                match maybe_event {
                    Some(Ok(event)) => match event {
                        Event::Key(key) => Some(TerminalEvent::Key(key)),
                        Event::Mouse(mouse) => Some(TerminalEvent::Mouse(mouse)),
                        Event::Resize(w, h) => Some(TerminalEvent::Resize(w, h)),
                        _ => None,
                    },
                    _ => None,
                }
            }
            // Periodic tick.
            _ = self.tick_interval.tick() => {
                Some(TerminalEvent::Tick)
            }
        }
    }
}
