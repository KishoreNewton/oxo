# oxo Architecture

This document describes the high-level architecture of oxo, the terminal UI
for log aggregation and observability.

## Crate structure

oxo is organized as a Cargo workspace with four crates:

```
┌──────────────────────────────────────────────────┐
│                   oxo (cli)                       │
│         Binary entry point, config, wiring        │
├──────────────────────────────────────────────────┤
│                                                   │
│  ┌─────────────┐    ┌────────────────────────┐   │
│  │  oxo-loki   │    │      oxo-tui           │   │
│  │  (backend)  │    │  (terminal interface)   │   │
│  └──────┬──────┘    └───────────┬────────────┘   │
│         │                       │                 │
│         └──────────┬────────────┘                 │
│                    │                              │
│            ┌───────▼────────┐                     │
│            │   oxo-core     │                     │
│            │ (traits/types) │                     │
│            └────────────────┘                     │
│                                                   │
└──────────────────────────────────────────────────┘
```

### oxo-core

The stable interface contract. Defines:

- **`LogBackend` trait** — what every backend must implement
- **`LogEntry`** — the normalized log entry type consumed by the TUI
- **`BackendError`** — unified error type
- **`BackendEvent`** — events flowing from backends to the UI
- **`TimeRange`** — time range for historical queries
- **`AppConfig`** — configuration structures

This crate has no network or rendering dependencies. Backend crates and
the TUI crate both depend only on `oxo-core`.

### oxo-loki

The Grafana Loki backend. Implements `LogBackend` by:

- Querying historical logs via `GET /loki/api/v1/query_range`
- Live tailing via WebSocket `GET /loki/api/v1/tail`
- Label discovery via `GET /loki/api/v1/labels` and `/label/{name}/values`
- Health checking via `GET /ready`

Handles authentication (Basic, Bearer), multi-tenant (`X-Scope-OrgID`),
and automatic WebSocket reconnection with exponential backoff.

### oxo-tui

All UI rendering and input handling. Key modules:

- **`app`** — The central `App` struct and async event loop
- **`components/`** — Self-contained UI components:
  - `log_viewer` — scrollable log display with tail mode
  - `query_bar` — text input with history
  - `filter_panel` — label-based filtering sidebar
  - `sparkline` — log rate visualization
  - `status_bar` — connection status and throughput
  - `help` — keyboard shortcut overlay
- **`layout`** — Panel arrangement and focus management
- **`event`** — Terminal event stream (keys, mouse, resize, tick)
- **`keymap`** — Key binding definitions (vim-style)
- **`theme`** — Color palette
- **`terminal`** — Terminal setup/teardown

### oxo (cli)

Thin binary that wires everything together:

1. Parses CLI arguments (clap)
2. Loads config from file + CLI overrides
3. Constructs the appropriate backend
4. Creates `App` and runs the event loop
5. Restores terminal on exit

## Data flow

### Live tailing

```
Loki WebSocket ──frame──► tail.rs ──deserialize──► LogEntry
                                                      │
                                          mpsc::UnboundedSender
                                                      │
                                                      ▼
App::run() tokio::select! ◄── mpsc::UnboundedReceiver
      │
      ├──► push to VecDeque<LogEntry> ring buffer
      ├──► update sparkline rate counter
      ├──► update status bar buffer count
      └──► on next render, draw frame
```

### Input handling

```
Terminal ──KeyEvent──► EventReader ──► App::handle_terminal_event()
                                           │
                                    ┌──────┴──────┐
                                    │ Focused      │
                                    │ Component    │
                                    │ handle_key() │
                                    └──────┬──────┘
                                           │
                                    ┌──────▼──────┐
                                    │  Action      │ (if component didn't handle)
                                    │  dispatch    │◄── keymap::handle_key()
                                    └──────┬──────┘
                                           │
                                    ┌──────▼──────┐
                                    │ Broadcast to │
                                    │ all comps    │
                                    └─────────────┘
```

## Component trait

All UI components implement:

```rust
pub trait Component {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action>;
    fn handle_action(&mut self, action: &Action) -> Option<Action>;
    fn render(&self, frame: &mut Frame, area: Rect, focused: bool);
}
```

This keeps components decoupled — they communicate only through `Action`
values, never by mutating each other's state.

## Adding a backend

See [adding-a-backend.md](adding-a-backend.md).
