# oxo

**A terminal UI for log aggregation and observability — k9s for your logs.**

<p align="center">
  <img src="docs/screenshot.png" alt="oxo screenshot" width="800" />
</p>

> [!NOTE]
> oxo is under active development. The first supported backend is Grafana Loki, with Elasticsearch and CloudWatch planned.

## Why oxo?

Observability SaaS tools like Datadog, Papertrail, and New Relic are powerful but expensive — often $50K+/year for growing teams. oxo gives you a fast, keyboard-driven interface to your existing log infrastructure, right in your terminal:

- **Real-time log tailing** with WebSocket streaming
- **LogQL query bar** with history and autocomplete
- **Label filtering** sidebar for quick drill-downs
- **Sparkline charts** showing log volume over time
- **SSH-friendly** — works over remote sessions and in tmux
- **Pluggable backends** — Loki today, Elasticsearch and CloudWatch next

Think of it as **k9s for observability**: all the power, none of the browser tabs.

## Installation

### From source (requires Rust 1.85+)

```sh
git clone https://github.com/oxo-tui/oxo.git
cd oxo
cargo install --path crates/oxo-cli
```

### With cargo

```sh
cargo install oxo
```

## Quick start

```sh
# Connect to a local Loki instance and start browsing
oxo --url http://localhost:3100

# Start tailing with a specific LogQL query
oxo --url http://loki:3100 --query '{job="api"} |= "error"'

# Use basic auth
oxo --url https://loki.example.com --username admin --password secret

# Use a bearer token (e.g. Grafana Cloud)
oxo --url https://logs-prod.grafana.net --token glc_...

# Multi-tenant Loki
oxo --url http://loki:3100 --org-id my-tenant
```

## Keyboard shortcuts

| Key         | Action                |
|-------------|----------------------|
| `j` / `↓`  | Scroll down          |
| `k` / `↑`  | Scroll up            |
| `g` / Home  | Jump to top          |
| `G` / End   | Jump to bottom (tail)|
| `Ctrl+d`    | Page down            |
| `Ctrl+u`    | Page up              |
| `/` or `:`  | Enter query mode     |
| `f`         | Toggle filter panel  |
| `w`         | Toggle line wrap     |
| `t`         | Toggle timestamps    |
| `y`         | Copy current line    |
| `Tab`       | Cycle focus          |
| `?`         | Toggle help          |
| `q` / `Ctrl+c` | Quit             |

## Configuration

oxo looks for a config file at `~/.config/oxo/config.toml`:

```toml
# Backend to use
backend = "loki"

[connection]
url = "http://localhost:3100"

# Optional: basic auth
# [connection.auth]
# type = "basic"
# username = "admin"
# password = "secret"

# Optional: bearer token
# [connection.auth]
# type = "bearer"
# token = "glc_..."

# Optional: Loki multi-tenant
# [connection.extra]
# org_id = "my-tenant"

[display]
max_buffer_size = 50000
mouse = true
tick_rate_ms = 250
show_timestamps = true
line_wrap = false
```

## Architecture

oxo is built as a Cargo workspace with four crates:

| Crate       | Purpose                                              |
|-------------|------------------------------------------------------|
| `oxo-core`  | Shared traits, types, and errors                     |
| `oxo-loki`  | Grafana Loki backend (HTTP + WebSocket)              |
| `oxo-tui`   | Terminal UI (ratatui + crossterm)                     |
| `oxo` (cli) | Binary entry point, config loading, backend wiring   |

The `LogBackend` trait in `oxo-core` is the interface contract. Adding a new backend means implementing this trait in a new crate — no TUI changes needed.

See [`docs/architecture.md`](docs/architecture.md) for details and [`docs/adding-a-backend.md`](docs/adding-a-backend.md) for a contributor guide.

## Supported backends

| Backend        | Status        | Crate         |
|---------------|---------------|---------------|
| Grafana Loki  | Supported     | `oxo-loki`    |
| Elasticsearch | Planned       | —             |
| CloudWatch    | Planned       | —             |
| OpenTelemetry | Planned       | —             |

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

Licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.
