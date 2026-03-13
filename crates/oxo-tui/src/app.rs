//! Application state machine and main event loop.
//!
//! The [`App`] struct ties everything together: it owns the backend, the
//! log buffer, all UI components, and runs the async event loop that
//! dispatches input to components and renders frames.

use std::collections::VecDeque;
use std::time::Duration;

use crossterm::event::{MouseButton, MouseEventKind};
use tokio::sync::mpsc;

use oxo_core::backend::LogBackend;
use oxo_core::config::DisplayConfig;
use oxo_core::{LogEntry, TailHandle};

use crate::action::Action;
use crate::components::Component;
use crate::components::detail_panel::DetailPanel;
use crate::components::filter_panel::FilterPanel;
use crate::components::help::HelpOverlay;
use crate::components::log_viewer::LogViewer;
use crate::components::query_bar::QueryBar;
use crate::components::search_bar::SearchBar;
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
    query_bar: QueryBar,
    log_viewer: LogViewer,
    filter_panel: FilterPanel,
    sparkline: SparklineChart,
    status_bar: StatusBar,
    help: HelpOverlay,
    search_bar: SearchBar,
    detail_panel: DetailPanel,

    /// Display configuration.
    display_config: DisplayConfig,

    /// The base query from filters (rebuilt when filters change).
    base_query: String,

    /// Pending query to start tailing on next loop iteration.
    pending_query: Option<String>,

    /// Notification message (auto-clears after a few ticks).
    notification: Option<(String, bool, u8)>, // (message, is_error, ticks_remaining)

    /// Whether the application should quit.
    should_quit: bool,
}

impl App {
    /// Create a new application instance.
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
            help: HelpOverlay::new(theme.clone()),
            search_bar: SearchBar::new(theme.clone()),
            detail_panel: DetailPanel::new(theme),
            display_config,
            base_query: String::new(),
            pending_query: None,
            notification: None,
            should_quit: false,
        }
    }

    /// Run the application main loop.
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

        // Fetch labels for the filter panel.
        self.load_labels().await;

        // Start tailing with the initial query.
        let query = self.query_bar.current_query().to_string();
        self.start_tail(&query).await;

        self.render(tui)?;

        loop {
            tokio::select! {
                Some(event) = events.next() => {
                    self.handle_terminal_event(event);
                }
                Some(entry) = self.tail_rx.recv() => {
                    self.handle_log_entry(entry);
                }
            }

            // Process pending tail start.
            if let Some(query) = self.pending_query.take() {
                self.start_tail(&query).await;
            }

            self.render(tui)?;

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// Handle a terminal event.
    fn handle_terminal_event(&mut self, event: TerminalEvent) {
        let action = match event {
            TerminalEvent::Key(key) => {
                // Detail panel captures all keys when visible.
                if self.detail_panel.is_visible() {
                    self.detail_panel.handle_key(key).unwrap_or(Action::Noop)
                }
                // Search bar captures keys when active.
                else if self.search_bar.is_active() {
                    self.search_bar
                        .handle_key(key)
                        .unwrap_or_else(|| keymap::handle_key(InputMode::Search, key))
                }
                // Otherwise route to focused component then global keymap.
                else {
                    let component_action = match self.focus.current() {
                        FocusTarget::QueryBar => self.query_bar.handle_key(key),
                        FocusTarget::LogViewer => self.log_viewer.handle_key(key),
                        FocusTarget::FilterPanel => self.filter_panel.handle_key(key),
                        FocusTarget::Sparkline => None,
                    };
                    component_action.unwrap_or_else(|| keymap::handle_key(self.input_mode, key))
                }
            }
            TerminalEvent::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => Action::MouseScrollUp(mouse.column, mouse.row),
                MouseEventKind::ScrollDown => Action::MouseScrollDown(mouse.column, mouse.row),
                MouseEventKind::Down(MouseButton::Left) => {
                    Action::MouseClick(mouse.column, mouse.row)
                }
                _ => Action::Noop,
            },
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

            // ── Mode switching ──────────────────────────────────────
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
            Action::EnterSearchMode => {
                self.input_mode = InputMode::Search;
                self.search_bar.activate();
            }
            Action::ExitSearchMode => {
                self.input_mode = InputMode::Normal;
                self.search_bar.deactivate();
            }

            // ── Query / filter ──────────────────────────────────────
            Action::SubmitQuery(query) => {
                self.input_mode = InputMode::Normal;
                let query = query.clone();
                self.base_query = query.clone();
                self.pending_start_tail(query);
            }
            Action::SetFilter { .. } => {
                // FilterPanel already toggled the filter internally.
                // Rebuild the query from base + active filters.
                self.rebuild_filter_query();
            }
            Action::ClearFilters => {
                self.rebuild_filter_query();
            }

            // ── Search ──────────────────────────────────────────────
            Action::SearchSubmit(_) | Action::SearchNext | Action::SearchPrev => {
                // Handled by log_viewer via broadcast below.
            }
            Action::SearchClear => {
                self.input_mode = InputMode::Normal;
                // Handled by log_viewer via broadcast below.
            }

            // ── Navigation ──────────────────────────────────────────
            Action::FocusNext => self.focus.next(),
            Action::FocusPrev => self.focus.prev(),
            Action::ToggleFilterPanel => {
                self.filter_panel.toggle();
                self.focus
                    .set_filter_visible(self.filter_panel.is_visible());
                if self.filter_panel.is_visible() {
                    self.input_mode = InputMode::Filter;
                    self.focus.set(FocusTarget::FilterPanel);
                } else {
                    self.input_mode = InputMode::Normal;
                    self.focus.set(FocusTarget::LogViewer);
                }
            }
            Action::ToggleHelp => {
                // Handled by help component via broadcast.
            }
            Action::ToggleDetail => {
                let entry = self.log_viewer.selected_entry().cloned();
                self.detail_panel.toggle(entry);
                if self.detail_panel.is_visible() {
                    self.input_mode = InputMode::Detail;
                } else {
                    self.input_mode = InputMode::Normal;
                }
            }

            // ── Copy ────────────────────────────────────────────────
            Action::CopyLine => {
                if let Some(entry) = self.log_viewer.selected_entry() {
                    let text = entry.line.clone();
                    match arboard::Clipboard::new() {
                        Ok(mut cb) => {
                            if cb.set_text(&text).is_ok() {
                                self.notification = Some(("Copied to clipboard".into(), false, 12));
                            } else {
                                self.notification =
                                    Some(("Clipboard write failed".into(), true, 12));
                            }
                        }
                        Err(_) => {
                            self.notification = Some(("Clipboard unavailable".into(), true, 12));
                        }
                    }
                } else {
                    self.notification =
                        Some(("No line selected (Space to select)".into(), false, 12));
                }
            }

            // ── Export ──────────────────────────────────────────────
            Action::ExportLogs => {
                self.export_logs();
            }

            // ── Mouse ───────────────────────────────────────────────
            Action::MouseScrollUp(_, _) => {
                self.log_viewer.handle_action(&Action::ScrollUp(3));
            }
            Action::MouseScrollDown(_, _) => {
                self.log_viewer.handle_action(&Action::ScrollDown(3));
            }
            Action::MouseClick(_x, _y) => {
                // Could map click position to component focus in the future.
            }

            // ── Notifications ───────────────────────────────────────
            Action::Notify(msg) => {
                self.notification = Some((msg.clone(), false, 12));
            }
            Action::NotifyError(msg) => {
                self.notification = Some((msg.clone(), true, 20));
            }

            // ── Tick ────────────────────────────────────────────────
            Action::Tick => {
                self.sparkline.tick();
                self.status_bar.set_rate(self.sparkline.current_rate());
                // Decrement notification timer.
                if let Some((_, _, ref mut ticks)) = self.notification {
                    if *ticks == 0 {
                        self.notification = None;
                    } else {
                        *ticks -= 1;
                    }
                }
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
        self._tail_handle = None;

        self.log_buffer.clear();
        self.log_viewer.update_entries(&self.log_buffer);
        self.status_bar.set_buffer_size(0);

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
                let msg = format!("Tail error: {e}");
                tracing::error!("{msg}");
                self.notification = Some((msg, true, 20));
            }
        }
    }

    /// Queue a query to start on the next event loop iteration.
    fn pending_start_tail(&mut self, query: String) {
        self.pending_query = Some(query);
    }

    /// Fetch labels from the backend and populate the filter panel.
    async fn load_labels(&mut self) {
        match self.backend.labels().await {
            Ok(labels) => {
                self.filter_panel.set_labels(labels);
            }
            Err(e) => {
                tracing::warn!("failed to load labels: {e}");
            }
        }
    }

    /// Rebuild the tail query from the base query + active filter selections.
    fn rebuild_filter_query(&mut self) {
        let filters = self.filter_panel.active_filters();
        if filters.is_empty() {
            let query = if self.base_query.is_empty() {
                "{}".to_string()
            } else {
                self.base_query.clone()
            };
            self.pending_start_tail(query);
        } else {
            // Build a stream selector from filters.
            let matchers: Vec<String> = filters
                .iter()
                .map(|f| format!(r#"{}="{}""#, f.label, f.value))
                .collect();
            let selector = format!("{{{}}}", matchers.join(", "));
            self.pending_start_tail(selector);
        }
    }

    /// Export visible log entries to a JSON file.
    fn export_logs(&mut self) {
        let filename = format!(
            "oxo-export-{}.json",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        );
        let entries: Vec<&LogEntry> = self.log_buffer.iter().collect();

        match serde_json::to_string_pretty(&entries) {
            Ok(json) => match std::fs::write(&filename, json) {
                Ok(()) => {
                    let msg = format!("Exported {} entries to {filename}", entries.len());
                    self.notification = Some((msg, false, 20));
                }
                Err(e) => {
                    self.notification = Some((format!("Export failed: {e}"), true, 20));
                }
            },
            Err(e) => {
                self.notification = Some((format!("Serialize failed: {e}"), true, 20));
            }
        }
    }

    /// Render all components to the terminal.
    fn render(&mut self, tui: &mut Tui) -> anyhow::Result<()> {
        tui.draw(|frame| {
            let area = frame.area();
            let layout = layout::compute_layout(area, self.filter_panel.is_visible());

            self.log_viewer
                .set_viewport_height(layout.log_viewer.height.saturating_sub(2) as usize);

            // Query bar.
            self.query_bar.render(
                frame,
                layout.query_bar,
                self.focus.is_focused(FocusTarget::QueryBar),
            );

            // Filter panel.
            if self.filter_panel.is_visible() {
                self.filter_panel.render(
                    frame,
                    layout.filter_panel,
                    self.focus.is_focused(FocusTarget::FilterPanel),
                );
            }

            // Log viewer.
            self.log_viewer.render(
                frame,
                layout.log_viewer,
                self.focus.is_focused(FocusTarget::LogViewer),
            );

            // Sparkline.
            self.sparkline.render(
                frame,
                layout.sparkline,
                self.focus.is_focused(FocusTarget::Sparkline),
            );

            // Status bar (or notification if active).
            if let Some((ref msg, is_error, _)) = self.notification {
                let style = if is_error {
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::White)
                        .bg(ratatui::style::Color::Red)
                } else {
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::Black)
                        .bg(ratatui::style::Color::Green)
                };
                let paragraph = ratatui::widgets::Paragraph::new(format!(" {msg}")).style(style);
                frame.render_widget(paragraph, layout.status_bar);
            } else {
                self.status_bar.render(frame, layout.status_bar, false);
            }

            // Search bar (overlays status bar when active).
            if self.search_bar.is_active() {
                self.search_bar.render(frame, layout.status_bar, true);
            }

            // Detail panel (overlay on top of log viewer).
            self.detail_panel.render(frame, area, false);

            // Help overlay (on top of everything).
            self.help.render(frame, area, false);
        })?;

        Ok(())
    }
}
