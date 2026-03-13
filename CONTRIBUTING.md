# Contributing to oxo

Thank you for your interest in contributing to oxo! This guide will help you
get started.

## Getting started

### Prerequisites

- Rust 1.85+ (install via [rustup](https://rustup.rs))
- A running Loki instance for integration testing (optional — see below)

### Building

```sh
git clone https://github.com/oxo-tui/oxo.git
cd oxo
cargo build
```

### Running

```sh
# Run against a local Loki
cargo run -- --url http://localhost:3100

# Run with debug logging
cargo run -- --url http://localhost:3100 --debug
# Logs are written to ~/.local/state/oxo/oxo.log
```

### Testing

```sh
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p oxo-core
cargo test -p oxo-loki
cargo test -p oxo-tui

# Run with output
cargo test -- --nocapture
```

### Code quality

```sh
# Format code
cargo fmt

# Run clippy
cargo clippy -- -D warnings

# Check documentation
cargo doc --no-deps --open
```

## Project structure

```
crates/
├── oxo-core/    # Shared traits, types, errors
├── oxo-loki/    # Grafana Loki backend
├── oxo-tui/     # Terminal UI components
└── oxo-cli/     # Binary entry point
```

See [`docs/architecture.md`](docs/architecture.md) for a detailed breakdown.

## How to contribute

### Reporting bugs

Open a [GitHub issue](https://github.com/oxo-tui/oxo/issues) with:

1. What you expected to happen
2. What actually happened
3. Steps to reproduce
4. Your environment (OS, terminal emulator, Rust version)

### Adding a new backend

See [`docs/adding-a-backend.md`](docs/adding-a-backend.md) for a full walkthrough.

### Submitting changes

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/my-feature`)
3. Make your changes
4. Ensure tests pass (`cargo test`)
5. Ensure code is formatted (`cargo fmt`)
6. Ensure clippy is happy (`cargo clippy -- -D warnings`)
7. Commit with a clear message
8. Push and open a pull request

### Commit messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add CloudWatch backend support
fix: handle WebSocket reconnection on network change
docs: update README with new keyboard shortcuts
refactor: extract common HTTP client logic
```

## Code of conduct

Be kind, be constructive, be respectful. We're all here to build something useful.

## License

By contributing to oxo, you agree that your contributions will be licensed
under the same dual MIT/Apache-2.0 license as the project.
