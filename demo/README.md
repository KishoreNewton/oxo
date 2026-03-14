# oxo Demo Recordings

Scripted terminal recordings using [VHS](https://github.com/charmbracelet/vhs)
that showcase oxo's capabilities. Each tape produces a GIF and WebM suitable
for documentation, README embedding, and social media.

## Prerequisites

| Tool | Purpose | Install |
|------|---------|---------|
| [vhs](https://github.com/charmbracelet/vhs) | Terminal recording | `paru -S vhs` / `brew install vhs` |
| [Docker](https://docker.com) | Loki demo (tape 02 only) | System package |
| Rust toolchain | Building oxo | [rustup.rs](https://rustup.rs) |

## Quick Start

```bash
# Record all demos (skips Loki tape by default)
./demo/record-all.sh

# Record all demos including the Docker/Loki tape
./demo/record-all.sh --loki

# Record specific tapes
./demo/record-all.sh 01 03 07
```

Generated files land in `demo/output/`.

## Tape Index

| # | File | Duration | What it shows |
|---|------|----------|---------------|
| 01 | `quickstart` | ~25s | First launch, log streaming, help overlay, basic scrolling |
| 02 | `live-tailing` | ~35s | Docker → Loki → real-time tail from 8 microservices |
| 03 | `query-filter` | ~40s | LogQL queries, autocomplete, filter sidebar, regex matching |
| 04 | `search-navigate` | ~40s | Vim navigation, `/` search, `n`/`N` jumping, bookmarks |
| 05 | `multi-source-tabs` | ~35s | Multiple tabs, different queries per tab, source picker |
| 06 | `analytics-stats` | ~40s | Statistics overlay, analytics dashboard, dedup, context lines |
| 07 | `structured-column` | ~35s | JSON parsing, column/table mode, detail panel, clipboard |
| 08 | `alerting` | ~35s | Alert engine, alert history, health dashboard, live metrics |
| 09 | `advanced-overlays` | ~45s | Regex playground, trace waterfall, incident timeline, NL query |
| 10 | `export-workflow` | ~35s | Full investigation flow: query → search → bookmark → export |

## Docker Stack (Tape 02)

The `docker-compose.yml` spins up:

- **Loki 3.0** — log aggregation backend on `:3100`
- **Log Generator** — pushes synthetic microservice logs via Loki's push API

Services simulated: `api-gateway`, `auth-service`, `order-service`,
`payment-service`, `user-service`, `notification-svc`, `inventory-svc`,
`search-service` across `prod` and `staging` namespaces.

```bash
# Start manually
docker compose -f demo/docker-compose.yml up -d

# Connect oxo
oxo --backend loki --url http://localhost:3100 --query '{job="microservices"}'

# Tear down
docker compose -f demo/docker-compose.yml down
```

## Customizing Recordings

Each `.tape` file is a plain-text VHS script. Common settings at the top:

```
Set FontSize 15          # Larger = more readable in small embeds
Set Width 1400           # Terminal width in pixels
Set Height 750           # Terminal height in pixels
Set Theme "Catppuccin Mocha"   # Any VHS theme name
Set TypingSpeed 50ms     # Character-by-character typing delay
```

To output MP4 instead of GIF, change the `Output` line:
```
Output ../output/01-quickstart.mp4
```

## Embedding in Documentation

```markdown
<!-- In README.md -->
![Quick Start](demo/output/01-quickstart.gif)

<!-- Multiple demos -->
| Feature | Demo |
|---------|------|
| Live Tailing | ![](demo/output/02-live-tailing.gif) |
| Query Power | ![](demo/output/03-query-filter.gif) |
```

## Recording a Single Tape

```bash
# Make sure oxo is in PATH
export PATH="$PWD/target/release:$PATH"
cargo build --release

# Record one tape
cd demo
vhs tapes/01-quickstart.tape
```

## File Structure

```
demo/
├── docker-compose.yml       # Loki + log generator stack
├── config/
│   ├── loki.yml             # Loki server configuration
│   └── oxo-demo.toml        # oxo config with alert rules + sources
├── log-generator/
│   ├── Dockerfile           # Alpine + curl + bash
│   └── generate.sh          # Pushes logs to Loki push API
├── tapes/
│   ├── 01-quickstart.tape   # → 10-export-workflow.tape
│   └── ...
├── output/                  # Generated GIFs + WebMs (gitignored)
├── record-all.sh            # Orchestration script
└── README.md                # This file
```
