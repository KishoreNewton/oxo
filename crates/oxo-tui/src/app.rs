//! Application state machine and main event loop.
//!
//! The [`App`] struct ties everything together: it owns the backend, the
//! log buffer, all UI components, and runs the async event loop that
//! dispatches input to components and renders frames.

use std::collections::VecDeque;
use std::time::Duration;

use crossterm::event::{MouseButton, MouseEventKind};
use tokio::sync::mpsc;

use oxo_alert::engine::AlertEvent;
use oxo_analytics::engine::AnalyticsSnapshot;
use oxo_core::backend::LogBackend;
use oxo_core::config::DisplayConfig;
use oxo_core::query::TimeRange;
use oxo_core::{LogEntry, TailHandle};

use crate::action::Action;
use crate::components::Component;
use crate::components::alert_panel::AlertPanel;
use crate::components::analytics_panel::{
    AnalyticsPanel, AnomalyInfo, AnomalySeverity, CorrelationInfo, EndpointInfo, PatternInfo,
};
use crate::components::autocomplete::AutocompletePopup;
use crate::components::detail_panel::DetailPanel;
use crate::components::diff_view::DiffView;
use crate::components::filter_panel::FilterPanel;
use crate::components::health_dashboard::HealthDashboard;
use crate::components::help::HelpOverlay;
use crate::components::histogram::Histogram;
use crate::components::incident_timeline::IncidentTimeline;
use crate::components::live_dashboard::LiveDashboard;
use crate::components::log_viewer::LogViewer;
use crate::components::nl_query::NlQuery;
use crate::components::query_bar::QueryBar;
use crate::components::regex_playground::RegexPlayground;
use crate::components::saved_views::SavedViewsPanel;
use crate::components::search_bar::SearchBar;
use crate::components::source_picker::{SourceEntry, SourcePicker};
use crate::components::sparkline::SparklineChart;
use crate::components::stats_panel::StatsPanel;
use crate::components::status_bar::{ConnectionState, StatusBar};
use crate::components::tab_bar::TabBar;
use crate::components::time_picker::TimePicker;
use crate::components::trace_waterfall::TraceWaterfall;
use crate::event::{EventReader, TerminalEvent};
use crate::export::{self, ExportFormat};
use crate::keymap::{self, InputMode};
use crate::layout::{self, FocusManager, FocusTarget};
use crate::saved_queries::SavedQueries;
use crate::session::Session;
use crate::terminal::{self, Tui};
use crate::theme::Theme;
use crate::views::SavedViews;

/// Factory function that creates a backend by name and connection config.
///
/// The CLI passes its `create_backend` logic through this type so the TUI
/// can switch backends at runtime without knowing about concrete backend types.
pub type BackendFactory =
    Box<dyn Fn(&str, &oxo_core::config::ConnectionConfig) -> anyhow::Result<Box<dyn LogBackend>>>;

/// Optional channel endpoints for connecting the alert and analytics engines.
///
/// When provided, the app clones each incoming log entry to the engines and
/// processes events/snapshots they emit.
#[derive(Default)]
pub struct EngineChannels {
    /// Sender for feeding log entries to the alert engine.
    pub alert_entry_tx: Option<mpsc::UnboundedSender<LogEntry>>,
    /// Receiver for alert events (fired rules, action results).
    pub alert_event_rx: Option<mpsc::UnboundedReceiver<AlertEvent>>,
    /// Sender for feeding log entries to the analytics engine.
    pub analytics_entry_tx: Option<mpsc::UnboundedSender<LogEntry>>,
    /// Receiver for periodic analytics snapshots.
    pub analytics_snapshot_rx: Option<mpsc::UnboundedReceiver<AnalyticsSnapshot>>,
}

/// Await the next value from an optional receiver, or pend forever if `None`.
async fn recv_or_pending<T>(rx: &mut Option<mpsc::UnboundedReceiver<T>>) -> Option<T> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

/// Per-tab async state: log buffer and the dedicated tail channel.
struct TabState {
    /// Buffered log entries for this tab.
    log_buffer: VecDeque<LogEntry>,
    /// Receiver end of the tail channel.
    tail_rx: mpsc::UnboundedReceiver<LogEntry>,
    /// Sender end — passed to the backend tail call.
    tail_tx: mpsc::UnboundedSender<LogEntry>,
    /// Handle that keeps the tail task alive.
    tail_handle: Option<TailHandle>,
    /// The query currently running for this tab.
    query: String,
}

impl TabState {
    fn new(capacity: usize) -> Self {
        let (tail_tx, tail_rx) = mpsc::unbounded_channel();
        Self {
            log_buffer: VecDeque::with_capacity(capacity),
            tail_rx,
            tail_tx,
            tail_handle: None,
            query: "{}".to_string(),
        }
    }
}

/// The main application state.
pub struct App {
    /// The active log backend.
    backend: Box<dyn LogBackend>,

    /// Maximum buffer capacity per tab.
    max_buffer_size: usize,

    /// Per-tab state (log buffer, tail channel, tail handle, query).
    tabs_state: Vec<TabState>,

    /// Index of the currently visible tab.
    active_tab: usize,

    /// The current input mode.
    input_mode: InputMode,

    /// Focus manager.
    focus: FocusManager,

    // ── Components ──────────────────────────────────────────────────
    query_bar: QueryBar,
    log_viewer: LogViewer,
    filter_panel: FilterPanel,
    sparkline: SparklineChart,
    histogram: Histogram,
    status_bar: StatusBar,
    help: HelpOverlay,
    search_bar: SearchBar,
    detail_panel: DetailPanel,
    time_picker: TimePicker,
    stats_panel: StatsPanel,
    tab_bar: TabBar,
    source_picker: SourcePicker,

    alert_panel: AlertPanel,
    analytics_panel: AnalyticsPanel,
    health_dashboard: HealthDashboard,
    trace_waterfall: TraceWaterfall,
    regex_playground: RegexPlayground,
    diff_view: DiffView,
    incident_timeline: IncidentTimeline,
    live_dashboard: LiveDashboard,
    nl_query: NlQuery,
    saved_views_panel: SavedViewsPanel,
    saved_views: SavedViews,

    /// WASM plugin registry (available when the `wasm` feature is enabled).
    #[cfg(feature = "wasm")]
    plugin_registry: Option<oxo_wasm::PluginRegistry>,
    #[allow(dead_code)] // Available for query bar integration.
    autocomplete: AutocompletePopup,

    /// Factory for creating backends when switching sources.
    backend_factory: Option<BackendFactory>,

    /// Configured sources.
    sources: Vec<oxo_core::config::SourceConfig>,

    /// Display configuration.
    display_config: DisplayConfig,

    /// The base query from filters (rebuilt when filters change).
    base_query: String,

    /// Pending query to start tailing on next loop iteration (active tab).
    pending_query: Option<String>,

    /// Pending tail start for a newly created tab: (tab_index, query).
    pending_new_tab_query: Option<(usize, String)>,

    /// Pending label whose values should be loaded from the backend.
    pending_label_load: Option<String>,

    /// Pending source switch (source name to switch to).
    pending_switch_source: Option<String>,

    /// Saved queries manager.
    saved_queries: SavedQueries,

    /// Active time range expressed in minutes (default: 60 = Last 1 hour).
    time_range_minutes: u64,

    /// Notification message (auto-clears after a few ticks).
    notification: Option<(String, bool, u8)>, // (message, is_error, ticks_remaining)

    /// Cached log viewer area rect (updated each render, used for mouse click mapping).
    log_viewer_area: ratatui::layout::Rect,

    // ── Engine channels ──────────────────────────────────────────
    /// Sender for feeding log entries to the alert engine.
    alert_entry_tx: Option<mpsc::UnboundedSender<LogEntry>>,
    /// Receiver for alert events.
    alert_event_rx: Option<mpsc::UnboundedReceiver<AlertEvent>>,
    /// Sender for feeding log entries to the analytics engine.
    analytics_entry_tx: Option<mpsc::UnboundedSender<LogEntry>>,
    /// Receiver for analytics snapshots.
    analytics_snapshot_rx: Option<mpsc::UnboundedReceiver<AnalyticsSnapshot>>,

    /// Whether alerts are currently muted.
    alert_muted: bool,

    /// Whether the application should quit.
    should_quit: bool,
}

impl App {
    /// Create a new application instance.
    ///
    /// `backend_factory` and `sources` are optional — when provided the user
    /// can press `b` to switch between sources at runtime.
    pub fn new(
        backend: Box<dyn LogBackend>,
        display_config: DisplayConfig,
        initial_query: Option<String>,
        backend_factory: Option<BackendFactory>,
        sources: Vec<oxo_core::config::SourceConfig>,
        engine_channels: EngineChannels,
    ) -> Self {
        let theme = Theme::default();
        let backend_name = backend.name().to_string();

        let initial_q = initial_query.unwrap_or_else(|| "{}".to_string());

        let mut first_tab = TabState::new(display_config.max_buffer_size);
        first_tab.query = initial_q.clone();

        let mut source_picker = SourcePicker::new(theme.clone());
        if !sources.is_empty() {
            let entries: Vec<SourceEntry> = sources
                .iter()
                .map(|cfg| SourceEntry {
                    name: cfg.name.clone(),
                    backend: cfg.resolved_type().to_string(),
                    url: cfg.url.clone(),
                })
                .collect();
            source_picker.set_sources(entries);
        }

        Self {
            backend,
            max_buffer_size: display_config.max_buffer_size,
            tabs_state: vec![first_tab],
            active_tab: 0,
            input_mode: InputMode::Normal,
            focus: FocusManager::new(),
            query_bar: QueryBar::new(theme.clone(), Some(initial_q)),
            log_viewer: LogViewer::new(theme.clone()),
            filter_panel: FilterPanel::new(theme.clone()),
            sparkline: SparklineChart::new(theme.clone()),
            histogram: Histogram::new(theme.clone()),
            status_bar: StatusBar::new(theme.clone(), backend_name, display_config.max_buffer_size),
            help: HelpOverlay::new(theme.clone()),
            search_bar: SearchBar::new(theme.clone()),
            detail_panel: DetailPanel::new(theme.clone()),
            time_picker: TimePicker::new(theme.clone()),
            stats_panel: StatsPanel::new(theme.clone()),
            tab_bar: TabBar::new(theme.clone()),
            alert_panel: AlertPanel::new(theme.clone()),
            analytics_panel: AnalyticsPanel::new(theme.clone()),
            health_dashboard: HealthDashboard::new(theme.clone()),
            trace_waterfall: TraceWaterfall::new(theme.clone()),
            regex_playground: RegexPlayground::new(theme.clone()),
            diff_view: DiffView::new(theme.clone()),
            incident_timeline: IncidentTimeline::new(theme.clone()),
            live_dashboard: LiveDashboard::new(theme.clone()),
            nl_query: NlQuery::new(theme.clone()),
            saved_views_panel: SavedViewsPanel::new(theme.clone()),
            saved_views: SavedViews::load(),
            #[cfg(feature = "wasm")]
            plugin_registry: oxo_wasm::PluginRegistry::new().ok(),
            autocomplete: AutocompletePopup::new(theme),
            source_picker,
            backend_factory,
            sources,
            display_config,
            base_query: String::new(),
            pending_query: None,
            pending_new_tab_query: None,
            pending_label_load: None,
            pending_switch_source: None,
            saved_queries: SavedQueries::load(),
            time_range_minutes: 60,
            notification: None,
            log_viewer_area: ratatui::layout::Rect::default(),
            alert_entry_tx: engine_channels.alert_entry_tx,
            alert_event_rx: engine_channels.alert_event_rx,
            analytics_entry_tx: engine_channels.analytics_entry_tx,
            analytics_snapshot_rx: engine_channels.analytics_snapshot_rx,
            alert_muted: false,
            should_quit: false,
        }
    }

    // ── Tab state accessors ──────────────────────────────────────────

    fn active_state(&self) -> &TabState {
        &self.tabs_state[self.active_tab]
    }

    fn active_state_mut(&mut self) -> &mut TabState {
        &mut self.tabs_state[self.active_tab]
    }

    // ── Public entry point ───────────────────────────────────────────

    /// Run the application main loop.
    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mut tui = terminal::init()?;
        let result = self.event_loop(&mut tui).await;
        terminal::restore()?;
        result
    }

    // ── Event loop ──────────────────────────────────────────────────

    /// The core async event loop.
    async fn event_loop(&mut self, tui: &mut Tui) -> anyhow::Result<()> {
        let tick_rate = Duration::from_millis(self.display_config.tick_rate_ms);
        let mut events = EventReader::new(tick_rate);

        // Fetch labels for the filter panel.
        self.load_labels().await;

        // Start tailing with the initial query on tab 0.
        let query = self.tabs_state[0].query.clone();
        self.start_tail_for(0, &query).await;

        self.render(tui)?;

        loop {
            tokio::select! {
                Some(event) = events.next() => {
                    self.handle_terminal_event(event);
                }
                Some(entry) = self.tabs_state[self.active_tab].tail_rx.recv() => {
                    let idx = self.active_tab;
                    self.handle_log_entry_for_tab(idx, entry);
                }
                Some(alert_event) = recv_or_pending(&mut self.alert_event_rx) => {
                    self.handle_alert_event(alert_event);
                }
                Some(snapshot) = recv_or_pending(&mut self.analytics_snapshot_rx) => {
                    self.handle_analytics_snapshot(snapshot);
                }
            }

            // Drain non-active tabs non-blocking so their buffers fill while
            // the user is viewing another tab.
            let n_tabs = self.tabs_state.len();
            for i in 0..n_tabs {
                if i == self.active_tab {
                    continue;
                }
                while let Ok(entry) = self.tabs_state[i].tail_rx.try_recv() {
                    let buf = &mut self.tabs_state[i].log_buffer;
                    buf.push_back(entry);
                    if buf.len() > self.max_buffer_size {
                        buf.pop_front();
                    }
                }
            }

            // Process a pending tail restart on the active tab.
            if let Some(query) = self.pending_query.take() {
                let idx = self.active_tab;
                self.start_tail_for(idx, &query).await;
            }

            // Process a pending tail start on a newly created tab.
            if let Some((tab_idx, query)) = self.pending_new_tab_query.take() {
                self.start_tail_for(tab_idx, &query).await;
            }

            // Process pending label value loads.
            if let Some(label) = self.pending_label_load.take() {
                self.load_label_values(&label).await;
            }

            // Process pending source switch.
            if let Some(name) = self.pending_switch_source.take() {
                self.switch_source(&name).await;
            }

            self.render(tui)?;

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    // ── Terminal event handling ──────────────────────────────────────

    /// Handle a terminal event.
    fn handle_terminal_event(&mut self, event: TerminalEvent) {
        let action = match event {
            TerminalEvent::Key(key) => {
                // Overlay components capture all keys when visible (ordered
                // by "most recently opened on top").
                if self.nl_query.is_visible() {
                    self.nl_query.handle_key(key).unwrap_or(Action::Noop)
                } else if self.regex_playground.is_visible() {
                    self.regex_playground
                        .handle_key(key)
                        .unwrap_or(Action::Noop)
                } else if self.live_dashboard.is_visible() {
                    self.live_dashboard.handle_key(key).unwrap_or(Action::Noop)
                } else if self.incident_timeline.is_visible() {
                    self.incident_timeline
                        .handle_key(key)
                        .unwrap_or(Action::Noop)
                } else if self.saved_views_panel.is_visible() {
                    self.saved_views_panel
                        .handle_key(key)
                        .unwrap_or(Action::Noop)
                } else if self.diff_view.is_visible() {
                    self.diff_view.handle_key(key).unwrap_or(Action::Noop)
                } else if self.trace_waterfall.is_visible() {
                    self.trace_waterfall.handle_key(key).unwrap_or(Action::Noop)
                } else if self.health_dashboard.is_visible() {
                    self.health_dashboard
                        .handle_key(key)
                        .unwrap_or(Action::Noop)
                } else if self.alert_panel.is_visible() {
                    self.alert_panel.handle_key(key).unwrap_or(Action::Noop)
                } else if self.analytics_panel.is_visible() {
                    self.analytics_panel.handle_key(key).unwrap_or(Action::Noop)
                }
                // Source picker captures all keys when visible.
                else if self.source_picker.is_visible() {
                    self.source_picker.handle_key(key).unwrap_or(Action::Noop)
                }
                // Stats panel captures all keys when visible.
                else if self.stats_panel.is_visible() {
                    self.stats_panel.handle_key(key).unwrap_or(Action::Noop)
                }
                // Time picker captures all keys when visible.
                else if self.time_picker.is_visible() {
                    self.time_picker.handle_key(key).unwrap_or(Action::Noop)
                }
                // Detail panel captures all keys when visible.
                else if self.detail_panel.is_visible() {
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
                        FocusTarget::Sparkline | FocusTarget::Histogram => None,
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

    /// Handle a log entry arriving for a specific tab.
    fn handle_log_entry_for_tab(&mut self, tab_idx: usize, entry: LogEntry) {
        // Run WASM plugins on the entry (transform + filter).
        #[cfg(feature = "wasm")]
        let entry = {
            let mut entries = vec![entry];
            if let Some(ref registry) = self.plugin_registry {
                entries = registry.apply_transforms(entries);
                entries = registry.apply_filters(entries);
            }
            match entries.into_iter().next() {
                Some(e) => e,
                None => return, // Entry was filtered out by a plugin.
            }
        };

        // Extract level before moving entry into buffer.
        let level_owned = entry.labels.get("level").cloned();

        // Feed to alert and analytics engines (clone before moving into buffer).
        if let Some(tx) = &self.alert_entry_tx {
            let _ = tx.send(entry.clone());
        }
        if let Some(tx) = &self.analytics_entry_tx {
            let _ = tx.send(entry.clone());
        }

        let buf = &mut self.tabs_state[tab_idx].log_buffer;
        buf.push_back(entry);
        if buf.len() > self.max_buffer_size {
            buf.pop_front();
        }

        // Only update visible components when it is the active tab.
        if tab_idx == self.active_tab {
            let buf = &self.tabs_state[tab_idx].log_buffer;
            self.sparkline.record_entries(1);
            self.histogram.record_entry(level_owned.as_deref());
            self.log_viewer.update_entries(buf);
            self.status_bar.set_buffer_size(buf.len());
        }
    }

    // ── Action dispatch ──────────────────────────────────────────────

    /// Dispatch an action through the application.
    fn dispatch_action(&mut self, action: Action) {
        match &action {
            Action::Quit => {
                self.save_session();
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
                self.active_state_mut().query = query.clone();
                self.pending_start_tail(query);
            }
            Action::SetFilter { .. } => {
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

            // ── Tabs ────────────────────────────────────────────────
            Action::NewTab => {
                let query = "{}".to_string();
                if let Some(new_idx) = self.tab_bar.add_tab(query.clone()) {
                    // Create backing state for the new tab.
                    let mut ts = TabState::new(self.max_buffer_size);
                    ts.query = query.clone();
                    self.tabs_state.push(ts);

                    // Switch to it.
                    self.active_tab = new_idx;
                    self.tab_bar.set_active(new_idx);

                    // Clear log viewer — new tab has no entries yet.
                    let empty: VecDeque<LogEntry> = VecDeque::new();
                    self.log_viewer.update_entries(&empty);
                    self.status_bar.set_buffer_size(0);

                    // Queue tail start for next loop iteration.
                    self.pending_new_tab_query = Some((new_idx, query));
                } else {
                    self.notification = Some(("Maximum number of tabs reached".into(), false, 12));
                }
            }

            Action::CloseTab => {
                if self.tab_bar.tab_count() <= 1 {
                    self.notification = Some(("Cannot close the last tab".into(), false, 12));
                } else {
                    let close_idx = self.active_tab;

                    // Drop the tail handle for the closing tab.
                    self.tabs_state[close_idx].tail_handle = None;

                    // Remove the backing state.
                    self.tabs_state.remove(close_idx);

                    // Remove from bar (which re-numbers labels).
                    self.tab_bar.close_tab(close_idx);

                    // Clamp active_tab.
                    if self.active_tab >= self.tabs_state.len() {
                        self.active_tab = self.tabs_state.len() - 1;
                    }
                    self.tab_bar.set_active(self.active_tab);

                    // Sync UI to the now-active tab.
                    self.sync_to_active_tab();
                }
            }

            Action::SwitchTab(n) => {
                let n = *n;
                if n < self.tabs_state.len() {
                    self.active_tab = n;
                    self.tab_bar.set_active(n);
                    self.sync_to_active_tab();
                }
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
            Action::ToggleTimePicker => {
                self.time_picker.toggle();
            }
            Action::SetTimeRange(minutes) => {
                self.time_range_minutes = *minutes;
                let label = self.time_picker.selected_label();
                self.notification = Some((format!("Time range: {label}"), false, 12));
                // Restart the tail so the historical backfill uses the new range.
                let query = self.active_state().query.clone();
                self.pending_start_tail(query);
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

            // ── Multi-line / context ────────────────────────────────
            Action::ToggleExpand => {
                self.log_viewer.toggle_expand();
            }
            Action::ToggleContext => {
                let ctx = self.log_viewer.context_lines();
                self.log_viewer.toggle_context();
                let new_ctx = self.log_viewer.context_lines();
                if new_ctx == 0 {
                    self.notification = Some(("Context view: off".into(), false, 12));
                } else {
                    self.notification = Some((format!("Context view: {new_ctx} lines"), false, 12));
                }
                let _ = ctx; // suppress unused warning
            }

            // ── Alerts ────────────────────────────────────────────────
            Action::AlertFired { rule_name, message } => {
                self.alert_panel
                    .push_alert(chrono::Utc::now(), rule_name.clone(), message.clone());
                if !self.alert_muted {
                    self.notification = Some((format!("Alert: {rule_name}"), true, 20));
                }
            }
            Action::ToggleAlertPanel => {
                self.alert_panel.toggle();
            }
            Action::ToggleAlertMute => {
                self.alert_muted = !self.alert_muted;
                self.alert_panel.set_muted(self.alert_muted);
                let state = if self.alert_muted { "muted" } else { "unmuted" };
                self.notification = Some((format!("Alerts {state}"), false, 12));
            }

            // ── Analytics ────────────────────────────────────────────
            Action::ToggleAnalytics => {
                self.analytics_panel.toggle();
            }

            // ── Health dashboard ──────────────────────────────────────
            Action::ToggleHealthDashboard => {
                self.health_dashboard.toggle();
                if self.health_dashboard.is_visible() {
                    // Populate with current metrics.
                    self.health_dashboard.backend_name = self.backend.name().to_string();
                    self.health_dashboard.entries_received =
                        self.active_state().log_buffer.len() as u64;
                    self.health_dashboard.entries_per_second = self.sparkline.current_rate() as f64;
                }
            }

            // ── Trace waterfall ──────────────────────────────────────
            Action::ToggleTraceWaterfall => {
                self.trace_waterfall.toggle();
                if self.trace_waterfall.is_visible() {
                    let entries: Vec<LogEntry> =
                        self.active_state().log_buffer.iter().cloned().collect();
                    self.trace_waterfall.build_from_entries(&entries);
                }
            }

            // ── Regex playground ──────────────────────────────────────
            Action::ToggleRegexPlayground => {
                self.regex_playground.toggle();
                if self.regex_playground.is_visible() {
                    let lines: Vec<String> = self
                        .active_state()
                        .log_buffer
                        .iter()
                        .map(|e| e.line.clone())
                        .collect();
                    self.regex_playground.set_lines(lines);
                }
            }

            // ── Diff mode ────────────────────────────────────────────
            Action::ToggleDiffMode => {
                self.diff_view.toggle();
            }
            Action::DiffQueryLeft(query) => {
                // Execute query and feed results to left pane.
                let query = query.clone();
                let range =
                    TimeRange::last(chrono::Duration::minutes(self.time_range_minutes as i64));
                let limit = self.max_buffer_size;
                // Store for async execution.
                self.notification = Some(("Diff: querying left...".to_string(), false, 8));
                // We can't do async in dispatch, so we'll do a best-effort with current buffer.
                // In a full implementation this would use pending_diff_query pattern.
                let entries: Vec<LogEntry> = self
                    .active_state()
                    .log_buffer
                    .iter()
                    .filter(|e| e.line.contains(&query))
                    .cloned()
                    .collect();
                self.diff_view.set_left_entries(entries);
                let _ = (range, limit); // Suppress unused warnings.
            }
            Action::DiffQueryRight(query) => {
                let query = query.clone();
                let entries: Vec<LogEntry> = self
                    .active_state()
                    .log_buffer
                    .iter()
                    .filter(|e| e.line.contains(&query))
                    .cloned()
                    .collect();
                self.diff_view.set_right_entries(entries);
            }

            // ── Saved views ──────────────────────────────────────────
            Action::ToggleSavedViews => {
                self.saved_views_panel.toggle();
                if self.saved_views_panel.is_visible() {
                    self.saved_views_panel
                        .set_views(self.saved_views.views.clone());
                }
            }

            // ── Live dashboard ──────────────────────────────────────
            Action::ToggleLiveDashboard => {
                self.live_dashboard.toggle();
            }

            // ── Incident timeline ──────────────────────────────────
            Action::ToggleIncidentTimeline => {
                self.incident_timeline.toggle();
            }
            Action::MarkIncident(title) => {
                self.incident_timeline.mark_incident(title.clone());
                self.notification = Some(("Incident marked".into(), false, 12));
            }

            // ── Natural language query ──────────────────────────────
            Action::ToggleNlQuery => {
                self.nl_query.toggle();
            }

            // ── Export CSV / NDJSON ──────────────────────────────────
            Action::ExportCsv => {
                self.export_with_format(ExportFormat::Csv);
            }
            Action::ExportNdjson => {
                self.export_with_format(ExportFormat::Ndjson);
            }

            // ── Statistics ──────────────────────────────────────────
            Action::ToggleStats => {
                self.stats_panel.toggle();
                if self.stats_panel.is_visible() {
                    let entries: Vec<_> = self.active_state().log_buffer.iter().cloned().collect();
                    self.stats_panel.update_stats(&entries);
                }
            }

            // ── Sources ──────────────────────────────────────────────
            Action::ToggleSourcePicker => {
                if self.source_picker.has_sources() {
                    self.source_picker.toggle();
                } else {
                    self.notification = Some((
                        "No sources configured. Add [sources] to config.toml".into(),
                        false,
                        16,
                    ));
                }
            }
            Action::SwitchSource(name) => {
                self.pending_switch_source = Some(name.clone());
            }

            // ── Label values ────────────────────────────────────────
            Action::LoadLabelValues(label) => {
                self.pending_label_load = Some(label.clone());
            }

            // ── Saved queries ───────────────────────────────────────
            Action::SaveQuery(_) => {
                let query = self.query_bar.current_query().to_string();
                if !query.is_empty() {
                    let name = format!("query-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                    self.saved_queries.add(name.clone(), query);
                    match self.saved_queries.save() {
                        Ok(()) => {
                            self.notification =
                                Some((format!("Saved query as '{name}'"), false, 16));
                        }
                        Err(e) => {
                            self.notification = Some((format!("Save failed: {e}"), true, 20));
                        }
                    }
                } else {
                    self.notification = Some(("No query to save".into(), false, 12));
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
            Action::MouseClick(_x, y) => {
                // Map click position to a log line within the log viewer area.
                let y = *y;
                let area = self.log_viewer_area;
                // The log viewer block has a 1-row top border; content starts at area.y + 1.
                let content_top = area.y + 1;
                let content_bottom = area.y + area.height.saturating_sub(1);
                if y >= content_top && y < content_bottom {
                    let visible_index = (y - content_top) as usize;
                    self.log_viewer.select_line(visible_index);
                    self.focus.set(FocusTarget::LogViewer);
                }
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
                self.histogram.handle_action(&Action::Tick);
                self.status_bar.set_rate(self.sparkline.current_rate());
                // Keep health dashboard metrics fresh while it's open.
                if self.health_dashboard.is_visible() {
                    self.health_dashboard.entries_received =
                        self.active_state().log_buffer.len() as u64;
                    self.health_dashboard.entries_per_second = self.sparkline.current_rate() as f64;
                }
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

    // ── Tail management ─────────────────────────────────────────────

    /// Start (or restart) a live tail for the specified tab index.
    ///
    /// First fetches historical entries for the configured time range via
    /// `backend.query()`, then begins a live tail stream.
    async fn start_tail_for(&mut self, tab_idx: usize, query: &str) {
        // Drop existing handle — stops the old tail task.
        self.tabs_state[tab_idx].tail_handle = None;

        // Clear the buffer for this tab.
        self.tabs_state[tab_idx].log_buffer.clear();

        // Create a fresh channel for the new tail.
        let (tx, rx) = mpsc::unbounded_channel();
        self.tabs_state[tab_idx].tail_tx = tx.clone();
        self.tabs_state[tab_idx].tail_rx = rx;

        if tab_idx == self.active_tab {
            let empty: VecDeque<LogEntry> = VecDeque::new();
            self.log_viewer.update_entries(&empty);
            self.status_bar.set_buffer_size(0);
        }

        // Fetch historical entries for the configured time range.
        let range = TimeRange::last(chrono::Duration::minutes(self.time_range_minutes as i64));
        match self.backend.query(query, range, self.max_buffer_size).await {
            Ok(entries) => {
                let buf = &mut self.tabs_state[tab_idx].log_buffer;
                for entry in entries {
                    buf.push_back(entry);
                    if buf.len() > self.max_buffer_size {
                        buf.pop_front();
                    }
                }
                if tab_idx == self.active_tab {
                    let buf = &self.tabs_state[tab_idx].log_buffer;
                    self.log_viewer.update_entries(buf);
                    self.status_bar.set_buffer_size(buf.len());
                }
                tracing::info!(
                    "loaded {} historical entries for tab {tab_idx} (range: {}m)",
                    self.tabs_state[tab_idx].log_buffer.len(),
                    self.time_range_minutes,
                );
            }
            Err(e) => {
                tracing::warn!("historical query failed for tab {tab_idx}: {e}");
            }
        }

        // Start the live tail stream.
        match self.backend.tail(query, tx).await {
            Ok(handle) => {
                self.tabs_state[tab_idx].tail_handle = Some(handle);
                if tab_idx == self.active_tab {
                    self.status_bar
                        .set_connection_state(ConnectionState::Connected);
                }
                tracing::info!("tail started for tab {tab_idx} query: {query}");
            }
            Err(e) => {
                if tab_idx == self.active_tab {
                    self.status_bar
                        .set_connection_state(ConnectionState::Disconnected);
                }
                let msg = format!("Tail error (tab {}): {e}", tab_idx + 1);
                tracing::error!("{msg}");
                self.notification = Some((msg, true, 20));
            }
        }
    }

    /// Queue a query restart on the **active** tab for the next loop iteration.
    fn pending_start_tail(&mut self, query: String) {
        self.pending_query = Some(query);
    }

    // ── Tab helpers ──────────────────────────────────────────────────

    /// Synchronize the log viewer, sparkline, and status bar to the currently
    /// active tab's state.
    fn sync_to_active_tab(&mut self) {
        let idx = self.active_tab;
        let buf = &self.tabs_state[idx].log_buffer;
        self.log_viewer.update_entries(buf);
        self.status_bar.set_buffer_size(buf.len());
        // Update base_query to reflect the active tab's query.
        self.base_query = self.tabs_state[idx].query.clone();
    }

    // ── Source switching ───────────────────────────────────────────

    /// Switch to a different configured source by name.
    async fn switch_source(&mut self, name: &str) {
        // Find the source config.
        let source_cfg = match self.sources.iter().find(|s| s.name == name) {
            Some(cfg) => cfg.clone(),
            None => {
                self.notification = Some((format!("Unknown source: {name}"), true, 16));
                return;
            }
        };

        // Create the new backend via the factory.
        let factory = match &self.backend_factory {
            Some(f) => f,
            None => {
                self.notification = Some(("Backend factory not available".into(), true, 16));
                return;
            }
        };

        match factory(
            source_cfg.resolved_type(),
            &source_cfg.to_connection_config(),
        ) {
            Ok(new_backend) => {
                // Stop all existing tails.
                for ts in &mut self.tabs_state {
                    ts.tail_handle = None;
                    ts.log_buffer.clear();
                }

                // Swap the backend.
                self.backend = new_backend;
                let backend_name = self.backend.name().to_string();
                self.status_bar
                    .set_connection_state(ConnectionState::Disconnected);

                // Update status bar with new backend name.
                self.status_bar =
                    StatusBar::new(Theme::default(), backend_name.clone(), self.max_buffer_size);

                // Update source picker active state.
                self.source_picker.set_active_by_name(name);

                // Reload labels from the new backend.
                self.load_labels().await;

                // Restart tail on the active tab.
                let query = self.tabs_state[self.active_tab].query.clone();
                self.start_tail_for(self.active_tab, &query).await;

                // Sync viewer.
                self.sync_to_active_tab();

                self.notification =
                    Some((format!("Switched to {name} ({backend_name})"), false, 16));
            }
            Err(e) => {
                self.notification = Some((format!("Failed to switch: {e}"), true, 20));
            }
        }
    }

    // ── Labels / filters ────────────────────────────────────────────

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

    /// Fetch values for a specific label and populate the filter panel.
    async fn load_label_values(&mut self, label: &str) {
        match self.backend.label_values(label).await {
            Ok(values) => {
                self.filter_panel.set_label_values(label, values);
            }
            Err(e) => {
                tracing::warn!("failed to load values for label '{label}': {e}");
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
            // Build a stream selector from filters (escape special chars in values).
            let matchers: Vec<String> = filters
                .iter()
                .map(|f| {
                    let escaped = f.value.replace('\\', r"\\").replace('"', r#"\""#);
                    format!(r#"{}="{}""#, f.label, escaped)
                })
                .collect();
            let selector = format!("{{{}}}", matchers.join(", "));
            self.pending_start_tail(selector);
        }
    }

    // ── Export ──────────────────────────────────────────────────────

    /// Export the active tab's log entries to a JSON file.
    fn export_logs(&mut self) {
        self.export_with_format(ExportFormat::Json);
    }

    /// Export the active tab's log entries in the given format.
    fn export_with_format(&mut self, format: ExportFormat) {
        let filename = format!(
            "oxo-export-{}.{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            format.extension()
        );
        let entries: Vec<&LogEntry> = self.active_state().log_buffer.iter().collect();

        match export::export_entries(&entries, format, &filename) {
            Ok(count) => {
                let msg = format!("Exported {count} entries to {filename}");
                self.notification = Some((msg, false, 20));
            }
            Err(e) => {
                self.notification = Some((format!("Export failed: {e}"), true, 20));
            }
        }
    }

    /// Save the current session state for restoration on next launch.
    fn save_session(&self) {
        let session = Session {
            tab_queries: self.tabs_state.iter().map(|ts| ts.query.clone()).collect(),
            active_tab: self.active_tab,
            time_range_minutes: self.time_range_minutes,
            active_source: None,
            filters: self
                .filter_panel
                .active_filters()
                .iter()
                .map(|f| (f.label.clone(), f.value.clone()))
                .collect(),
        };
        if let Err(e) = session.save() {
            tracing::warn!("failed to save session: {e}");
        }
    }

    // ── Alert / Analytics event handling ────────────────────────

    /// Handle an event from the alert engine.
    fn handle_alert_event(&mut self, event: AlertEvent) {
        match event {
            AlertEvent::Fired { rule_name, message } => {
                self.dispatch_action(Action::AlertFired { rule_name, message });
            }
            AlertEvent::ActionResult {
                rule_name,
                action_type,
                success,
                error,
            } => {
                if !success {
                    if let Some(err) = error {
                        tracing::warn!(
                            "alert action {action_type} for rule '{rule_name}' failed: {err}"
                        );
                    }
                }
            }
        }
    }

    /// Handle an analytics snapshot from the analytics engine.
    fn handle_analytics_snapshot(&mut self, snapshot: AnalyticsSnapshot) {
        // Patterns.
        let patterns: Vec<PatternInfo> = snapshot
            .top_patterns
            .iter()
            .map(|p| PatternInfo {
                template: p.template.clone(),
                count: p.count,
                example: p.example.clone(),
            })
            .collect();
        self.analytics_panel.set_patterns(patterns);

        // Anomalies.
        let mut anomalies: Vec<AnomalyInfo> = Vec::new();
        for a in &snapshot.anomalies {
            anomalies.push(AnomalyInfo {
                description: format!(
                    "Volume spike: {:.1}x above average (z-score: {:.1})",
                    a.actual_rate / a.expected_rate.max(1.0),
                    a.z_score
                ),
                timestamp: a.timestamp.format("%H:%M:%S").to_string(),
                severity: AnomalySeverity::VolumeSpike,
            });
        }
        for np in &snapshot.new_patterns {
            anomalies.push(AnomalyInfo {
                description: format!("New pattern: {}", np.template),
                timestamp: np.first_seen.format("%H:%M:%S").to_string(),
                severity: AnomalySeverity::NewPattern,
            });
        }
        self.analytics_panel.set_anomalies(anomalies);

        // Correlations.
        if let Some(ref corr) = snapshot.correlation {
            let correlations: Vec<CorrelationInfo> = corr
                .top_changes
                .iter()
                .map(|c| CorrelationInfo {
                    label: c.label.clone(),
                    value: c.value.clone(),
                    baseline: c.baseline_error_rate,
                    current: c.current_error_rate,
                    change: c.change_factor,
                })
                .collect();
            self.analytics_panel.set_correlations(correlations);
        }

        // Trends.
        if let Some(ref trend) = snapshot.trend {
            let direction = if trend.slope > 0.001 {
                "increasing"
            } else if trend.slope < -0.001 {
                "decreasing"
            } else {
                "stable"
            };
            let desc = format!(
                "Error rate: slope={:.4}/min, R²={:.3} — {direction}",
                trend.slope, trend.r_squared,
            );
            let data: Vec<f64> = trend.data_points.iter().map(|(_, v)| *v).collect();
            self.analytics_panel.set_trend(desc, data);
        }

        // Endpoints.
        let endpoints: Vec<EndpointInfo> = snapshot
            .slowest_endpoints
            .iter()
            .map(|e| EndpointInfo {
                pattern: e.pattern.clone(),
                p50: e.p50_ms,
                p95: e.p95_ms,
                p99: e.p99_ms,
                count: e.sample_count,
            })
            .collect();
        self.analytics_panel.set_endpoints(endpoints);

        // Noisy sources.
        self.analytics_panel
            .set_noisy_sources(snapshot.noisiest_sources);
    }

    // ── Render ──────────────────────────────────────────────────────

    /// Render all components to the terminal.
    fn render(&mut self, tui: &mut Tui) -> anyhow::Result<()> {
        tui.draw(|frame| {
            let area = frame.area();
            let layout = layout::compute_layout(area, self.filter_panel.is_visible());

            self.log_viewer
                .set_viewport_height(layout.log_viewer.height.saturating_sub(2) as usize);

            // Cache the log viewer area for mouse click mapping.
            self.log_viewer_area = layout.log_viewer;

            // Update tail/paused indicator on the status bar.
            self.status_bar
                .set_tail_mode(self.log_viewer.is_tail_mode());

            // Query bar.
            self.query_bar.render(
                frame,
                layout.query_bar,
                self.focus.is_focused(FocusTarget::QueryBar),
            );

            // Tab bar.
            self.tab_bar.render(frame, layout.tab_bar, false);

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

            // Histogram (replaces sparkline visually).
            self.histogram.render(
                frame,
                layout.histogram,
                self.focus.is_focused(FocusTarget::Histogram),
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

            // Time picker overlay.
            self.time_picker.render(frame, area, false);

            // Stats panel overlay.
            self.stats_panel.render(frame, area, false);

            // Alert panel overlay.
            self.alert_panel.render(frame, area, false);

            // Analytics panel overlay.
            self.analytics_panel.render(frame, area, false);

            // Health dashboard overlay.
            self.health_dashboard.render(frame, area, false);

            // Trace waterfall overlay.
            self.trace_waterfall.render(frame, area, false);

            // Diff view overlay.
            self.diff_view.render(frame, area, false);

            // Regex playground overlay.
            self.regex_playground.render(frame, area, false);

            // Incident timeline overlay.
            self.incident_timeline.render(frame, area, false);

            // Live dashboard overlay.
            self.live_dashboard.render(frame, area, false);

            // Saved views overlay.
            self.saved_views_panel.render(frame, area, false);

            // Natural language query overlay (topmost interactive).
            self.nl_query.render(frame, area, false);

            // Source picker overlay (topmost layer).
            self.source_picker.render(frame, area, false);
        })?;

        Ok(())
    }
}
