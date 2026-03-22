# oxo

**A terminal UI for log aggregation and observability — k9s for your logs.**

<p align="center">
  <img src="demo/output/01-quickstart.gif" alt="oxo quickstart demo" width="800" />
</p>

> [!NOTE]
> oxo is under active development. Supported backends: Grafana Loki, Elasticsearch, AWS CloudWatch, local files, Docker, and Kubernetes.

## Why oxo?

Observability SaaS tools like Datadog, Papertrail, and New Relic are powerful but expensive — often $50K+/year for growing teams. oxo gives you a fast, keyboard-driven interface to your existing log infrastructure, right in your terminal:

- **Real-time log tailing** with WebSocket streaming
- **LogQL query bar** with history and autocomplete
- **Label filtering** sidebar for quick drill-downs
- **Sparkline charts** showing log volume over time
- **SSH-friendly** — works over remote sessions and in tmux
- **Pluggable backends** — Loki, Elasticsearch, CloudWatch, local files, Docker, K8s

Think of it as **k9s for observability**: all the power, none of the browser tabs.

## Features in Action

### Analytics & Statistics Dashboard

Built-in stats overlay, analytics dashboard, and log deduplication — no Grafana needed.

<p align="center">
  <img src="demo/output/06-analytics-stats.gif" alt="Analytics and stats dashboard" width="800" />
</p>

### Structured Logs & Column Mode

Parse JSON logs into columns, expand detail panels, toggle timestamps and line wrapping.

<p align="center">
  <img src="demo/output/07-structured-column.gif" alt="Structured logs and column mode" width="800" />
</p>

### Health & Live Metrics

Health dashboard, live metrics, statistics overlay, and alert panel — all without leaving the terminal.

<p align="center">
  <img src="demo/output/08-alerting.gif" alt="Alerting and health dashboard" width="800" />
</p>

### Advanced Overlays

Regex playground, trace waterfall, incident timeline, natural language query, and saved views.

<p align="center">
  <img src="demo/output/09-advanced-overlays.gif" alt="Advanced overlays" width="800" />
</p>

### Export & Investigation Workflow

Complete investigation flow: query → filter → search → bookmark → export. Save queries, export as JSON, manage saved views.

<p align="center">
  <img src="demo/output/10-export-workflow.gif" alt="Export and investigation workflow" width="800" />
</p>

## Installation

### From source (requires Rust 1.85+)

```sh
git clone https://github.com/KishoreNewton/oxo.git
cd oxo
cargo install --path crates/oxo-cli
```

### With cargo (from source)

```sh
cargo install --git https://github.com/KishoreNewton/oxo.git oxo
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

| Key         | Action                          |
|-------------|---------------------------------|
| `j` / `↓`  | Scroll down                     |
| `k` / `↑`  | Scroll up                       |
| `g` / Home  | Jump to top                     |
| `G` / End   | Jump to bottom (tail)           |
| `Ctrl+d/u`  | Page down / up                  |
| `/`         | Search                          |
| `:`         | Enter query mode                |
| `f`         | Toggle filter panel             |
| `D`         | Dedup (Off → Exact → Fuzzy)     |
| `c`         | Column / table mode             |
| `s`         | Log statistics                  |
| `a`         | Alert panel                     |
| `i`         | Analytics dashboard             |
| `H`         | Health dashboard                |
| `L`         | Live metrics dashboard          |
| `m` / `'`   | Toggle / jump to bookmark       |
| `w`         | Toggle line wrap                |
| `t`         | Toggle timestamps               |
| `y`         | Copy current line               |
| `e`         | Export logs                     |
| `?`         | Toggle help                     |
| `q` / `Ctrl+c` | Quit                        |

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

oxo is built as a Cargo workspace with 11 crates:

| Crate              | Purpose                                              |
|--------------------|------------------------------------------------------|
| `oxo-core`         | Shared traits, types, pipeline, and errors           |
| `oxo-loki`         | Grafana Loki backend (HTTP + WebSocket)              |
| `oxo-elasticsearch`| Elasticsearch / OpenSearch backend                   |
| `oxo-cloudwatch`   | AWS CloudWatch Logs backend (SigV4)                  |
| `oxo-local`        | Local file, command, Docker, K8s, stdin backends     |
| `oxo-demo`         | Synthetic log generator for demos                    |
| `oxo-tui`          | Terminal UI (ratatui + crossterm, 25 components)     |
| `oxo-alert`        | Rule-based alerting engine                           |
| `oxo-analytics`    | Pattern clustering, anomaly detection, trend analysis|
| `oxo-wasm`         | WASM plugin system (wasmtime)                        |
| `oxo` (cli)        | Binary entry point, config loading, backend wiring   |

The `LogBackend` trait in `oxo-core` is the interface contract. Adding a new backend means implementing this trait in a new crate — no TUI changes needed.

See [`docs/architecture.md`](docs/architecture.md) for details and [`docs/adding-a-backend.md`](docs/adding-a-backend.md) for a contributor guide.

## Supported backends

| Backend            | Status        | Crate               |
|--------------------|---------------|----------------------|
| Grafana Loki       | Supported     | `oxo-loki`           |
| Elasticsearch      | Supported     | `oxo-elasticsearch`  |
| AWS CloudWatch     | Supported     | `oxo-cloudwatch`     |
| Local file/command  | Supported     | `oxo-local`          |
| Docker logs        | Supported     | `oxo-local`          |
| Kubernetes logs    | Supported     | `oxo-local`          |
| Stdin / pipe       | Supported     | `oxo-local`          |
| OpenTelemetry      | Planned       | —                    |

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

Licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.
