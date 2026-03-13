//! Application state machine and main event loop.
//!
//! The [`App`] struct ties everything together: it owns the backend, the
//! log buffer, all UI components, and runs the async event loop that
//! dispatches input to components and renders frames.
//!
//! ## Architecture
//!
//! ```text
//!  Terminal Events ──► EventReader ──► App::handle_terminal_event()
//!                                          │
//!  Backend Tail   ──► mpsc channel ──►     ├──► Action dispatch
//!                                          │        │
//!  Tick Timer     ──► EventReader ──►      │        ▼
//!                                          │   Component::handle_action()
//!                                          │        │
//!                                          ▼        ▼
//!                                     App::render() ──► Terminal
//! ```

use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::mpsc;

use oxo_core::backend::LogBackend;
use oxo_core::config::DisplayConfig;
use oxo_core::{LogEntry, TailHandle};

use crate::action::Action;
use crate::components::Component;
use crate::components::filter_panel::FilterPanel;
use crate::components::help::HelpOverlay;
use crate::components::log_viewer::LogViewer;
use crate::components::query_bar::QueryBar;
use crate::components::sparkline::SparklineChart;
use crate::components::status_bar::{ConnectionState, StatusBar};
use crate::event::{EventReader, TerminalEvent};
use crate::keymap::{self, InputMode};
use crate::layout::{self, FocusManager, FocusTarget};
use crate::terminal::{self, Tui};
use crate::theme::Theme;

/// The main application state.
pub struct App {
    /// The active log backend.
    backend: Box<dyn LogBackend>,

    /// Ring buffer of log entries.
    log_buffer: VecDeque<LogEntry>,

    /// Maximum buffer capacity.
    max_buffer_size: usize,

    /// Receiver for log entries from the tail stream.
    tail_rx: mpsc::UnboundedReceiver<LogEntry>,

    /// Sender for log entries (held so we can start new tails).
    tail_tx: mpsc::UnboundedSender<LogEntry>,

    /// Handle to the current tail task (if any).
    _tail_handle: Option<TailHandle>,

    /// The current input mode.
    input_mode: InputMode,

    /// Focus manager.
    focus: FocusManager,

    // ── Components ──────────────────────────────────────────────────
    /// Query input bar.
    query_bar: QueryBar,

    /// Main log viewer.
    log_viewer: LogViewer,

    /// Label filter panel.
    filter_panel: FilterPanel,

    /// Sparkline rate chart.
    sparkline: SparklineChart,

    /// Status bar.
    status_bar: StatusBar,

    /// Help overlay.
    help: HelpOverlay,

    /// Display configuration.
    display_config: DisplayConfig,

    /// A query waiting to be started on the next event loop iteration.
    ///
    /// This exists because `dispatch_action` is synchronous but starting
    /// a tail requires an async call. The event loop drains this field.
    pending_query: Option<String>,

    /// Whether the application should quit.
    should_quit: bool,
}

impl App {
    /// Create a new application instance.
    ///
    /// # Arguments
    ///
    /// * `backend` — The log backend to use.
    /// * `display_config` — Display settings (buffer size, tick rate, etc.).
    /// * `initial_query` — An optional query to start tailing immediately.
    pub fn new(
        backend: Box<dyn LogBackend>,
        display_config: DisplayConfig,
        initial_query: Option<String>,
    ) -> Self {
        let theme = Theme::default();
        let (tail_tx, tail_rx) = mpsc::unbounded_channel();

        let backend_name = backend.name().to_string();

        Self {
            backend,
            log_buffer: VecDeque::with_capacity(display_config.max_buffer_size),
            max_buffer_size: display_config.max_buffer_size,
            tail_rx,
            tail_tx,
            _tail_handle: None,
            input_mode: InputMode::Normal,
            focus: FocusManager::new(),
            query_bar: QueryBar::new(theme.clone(), initial_query),
            log_viewer: LogViewer::new(theme.clone()),
            filter_panel: FilterPanel::new(theme.clone()),
            sparkline: SparklineChart::new(theme.clone()),
            status_bar: StatusBar::new(theme.clone(), backend_name, display_config.max_buffer_size),
            help: HelpOverlay::new(theme),
            display_config,
            pending_query: None,
            should_quit: false,
        }
    }

    /// Run the application main loop.
    ///
    /// This is the entry point called by `main()`. It initializes the
    /// terminal, starts the event loop, and restores the terminal on exit.
    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mut tui = terminal::init()?;
        let result = self.event_loop(&mut tui).await;
        terminal::restore()?;
        result
    }

    /// The core async event loop.
    async fn event_loop(&mut self, tui: &mut Tui) -> anyhow::Result<()> {
        let tick_rate = Duration::from_millis(self.display_config.tick_rate_ms);
        let mut events = EventReader::new(tick_rate);

        // If there's an initial query, start tailing.
        if !self.query_bar.current_query().is_empty() {
            let query = self.query_bar.current_query().to_string();
            self.start_tail(&query).await;
        }

        // Initial render.
        self.render(tui)?;

        loop {
            tokio::select! {
                // Terminal events (keys, mouse, resize, tick).
                Some(event) = events.next() => {
                    self.handle_terminal_event(event);
                }
                // Log entries from the tail stream.
                Some(entry) = self.tail_rx.recv() => {
                    self.handle_log_entry(entry);
                }
            }

            // If a new query was submitted, start tailing it.
            if let Some(query) = self.pending_query.take() {
                self.start_tail(&query).await;
            }

            // Render the current state.
            self.render(tui)?;

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// Handle a terminal event (key press, resize, tick).
    fn handle_terminal_event(&mut self, event: TerminalEvent) {
        let action = match event {
            TerminalEvent::Key(key) => {
                // Let the focused component handle the key first.
                let component_action = match self.focus.current() {
                    FocusTarget::QueryBar => self.query_bar.handle_key(key),
                    FocusTarget::LogViewer => self.log_viewer.handle_key(key),
                    FocusTarget::FilterPanel => self.filter_panel.handle_key(key),
                    FocusTarget::Sparkline => None,
                };

                // If the component didn't handle it, use the global keymap.
                component_action.unwrap_or_else(|| keymap::handle_key(self.input_mode, key))
            }
            TerminalEvent::Mouse(_) => Action::Noop,
            TerminalEvent::Resize(w, h) => Action::Resize {
                width: w,
                height: h,
            },
            TerminalEvent::Tick => Action::Tick,
        };

        self.dispatch_action(action);
    }

    /// Handle a log entry arriving from the tail stream.
    fn handle_log_entry(&mut self, entry: LogEntry) {
        self.log_buffer.push_back(entry);
        if self.log_buffer.len() > self.max_buffer_size {
            self.log_buffer.pop_front();
        }

        self.sparkline.record_entries(1);
        self.log_viewer.update_entries(&self.log_buffer);
        self.status_bar.set_buffer_size(self.log_buffer.len());
    }

    /// Dispatch an action through the application.
    fn dispatch_action(&mut self, action: Action) {
        match &action {
            Action::Quit => {
                self.should_quit = true;
                return;
            }
            Action::EnterQueryMode => {
                self.input_mode = InputMode::Query;
                self.query_bar.activate();
                self.focus.set(FocusTarget::QueryBar);
            }
            Action::ExitQueryMode => {
                self.input_mode = InputMode::Normal;
                self.query_bar.deactivate();
                self.focus.set(FocusTarget::LogViewer);
            }
            Action::SubmitQuery(query) => {
                self.input_mode = InputMode::Normal;
                let query = query.clone();
                self.pending_start_tail(query);
            }
            Action::FocusNext => self.focus.next(),
            Action::FocusPrev => self.focus.prev(),
            Action::ToggleFilterPanel => {
                self.filter_panel.toggle();
                self.focus
                    .set_filter_visible(self.filter_panel.is_visible());
            }
            Action::Tick => {
                self.sparkline.tick();
                self.status_bar.set_rate(self.sparkline.current_rate());
            }
            _ => {}
        }

        // Broadcast to all components.
        self.log_viewer.handle_action(&action);
        self.sparkline.handle_action(&action);
        self.help.handle_action(&action);
        self.status_bar.handle_action(&action);
    }

    /// Start a live tail for the given query.
    async fn start_tail(&mut self, query: &str) {
        // Drop the old tail handle (cancels the previous tail).
        self._tail_handle = None;

        // Clear the buffer for the new query.
        self.log_buffer.clear();
        self.log_viewer.update_entries(&self.log_buffer);
        self.status_bar.set_buffer_size(0);

        // Create a new channel for this tail.
        let (tx, rx) = mpsc::unbounded_channel();
        self.tail_tx = tx.clone();
        self.tail_rx = rx;

        match self.backend.tail(query, tx).await {
            Ok(handle) => {
                self._tail_handle = Some(handle);
                self.status_bar
                    .set_connection_state(ConnectionState::Connected);
                tracing::info!("tail started for query: {query}");
            }
            Err(e) => {
                self.status_bar
                    .set_connection_state(ConnectionState::Disconnected);
                tracing::error!("failed to start tail: {e}");
            }
        }
    }

    /// Queue a query to start tailing on the next event loop iteration.
    ///
    /// This is needed because `dispatch_action` is synchronous but
    /// `start_tail` is async. The event loop checks `self.pending_query`
    /// after each iteration and calls `start_tail` if set.
    fn pending_start_tail(&mut self, query: String) {
        self.pending_query = Some(query);
    }

    /// Render all components to the terminal.
    fn render(&mut self, tui: &mut Tui) -> anyhow::Result<()> {
        tui.draw(|frame| {
            let area = frame.area();
            let layout = layout::compute_layout(area, self.filter_panel.is_visible());

            // Update viewport height for scroll calculations.
            self.log_viewer
                .set_viewport_height(layout.log_viewer.height.saturating_sub(2) as usize);

            // Render each component.
            self.query_bar.render(
                frame,
                layout.query_bar,
                self.focus.is_focused(FocusTarget::QueryBar),
            );

            if self.filter_panel.is_visible() {
                self.filter_panel.render(
                    frame,
                    layout.filter_panel,
                    self.focus.is_focused(FocusTarget::FilterPanel),
                );
            }

            self.log_viewer.render(
                frame,
                layout.log_viewer,
                self.focus.is_focused(FocusTarget::LogViewer),
            );

            self.sparkline.render(
                frame,
                layout.sparkline,
                self.focus.is_focused(FocusTarget::Sparkline),
            );

            self.status_bar.render(frame, layout.status_bar, false);

            // Help overlay renders last (on top of everything).
            self.help.render(frame, area, false);
        })?;

        Ok(())
    }
}
