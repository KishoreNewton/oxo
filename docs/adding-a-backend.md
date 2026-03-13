# Adding a new backend to oxo

This guide walks through adding a new log backend to oxo. The process is
designed to be straightforward: implement one trait, wire it up, done.

## Overview

1. Create a new crate `crates/oxo-{name}/`
2. Implement the `LogBackend` trait from `oxo-core`
3. Add a match arm in the CLI's `create_backend()` function
4. Add the crate to the workspace

## Step 1: Create the crate

```sh
mkdir -p crates/oxo-mybackend/src
```

Create `crates/oxo-mybackend/Cargo.toml`:

```toml
[package]
name = "oxo-mybackend"
description = "MyBackend support for the oxo observability TUI"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
oxo-core.workspace = true
tokio.workspace = true
async-trait.workspace = true
# ... your backend's SDK/HTTP dependencies
```

Add it to the workspace in the root `Cargo.toml`:

```toml
[workspace]
members = ["crates/*"]  # Already covers it if using glob
```

## Step 2: Implement the LogBackend trait

The trait is defined in `oxo-core/src/backend.rs`. Here's what you need
to implement:

```rust
use async_trait::async_trait;
use oxo_core::backend::{LogBackend, LogEntry, TailHandle};
use oxo_core::config::ConnectionConfig;
use oxo_core::error::BackendError;
use oxo_core::query::TimeRange;
use tokio::sync::mpsc;

pub struct MyBackend {
    // Your client state here
}

#[async_trait]
impl LogBackend for MyBackend {
    fn name(&self) -> &str {
        "MyBackend"
    }

    async fn query(
        &self,
        query: &str,
        range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        // Translate `query` into your backend's query language.
        // Convert results into Vec<LogEntry>.
        todo!()
    }

    async fn tail(
        &self,
        query: &str,
        tx: mpsc::UnboundedSender<LogEntry>,
    ) -> Result<TailHandle, BackendError> {
        // Spawn a background task that streams entries into `tx`.
        // Return a TailHandle wrapping the JoinHandle.
        let handle = tokio::spawn(async move {
            // Your streaming logic here.
            // Send entries via: tx.send(entry)
        });
        Ok(TailHandle::new(handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        // Return available label/field names for the filter panel.
        todo!()
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        // Return known values for a label.
        todo!()
    }

    async fn health(&self) -> Result<(), BackendError> {
        // Check connectivity to the backend.
        todo!()
    }

    fn from_config(config: &ConnectionConfig) -> Result<Self, BackendError> {
        // Construct your backend from the connection config.
        // Use config.url, config.auth, and config.extra for
        // backend-specific settings.
        todo!()
    }
}
```

### Key points

- **`LogEntry` normalization**: Convert your backend's response format into
  `LogEntry`. At minimum, set `timestamp`, `line`, and relevant `labels`.
  The `raw` field is optional (used for "inspect" mode).

- **`tail()` contract**: Spawn a background task, return immediately. The
  task should stream entries into the `mpsc::UnboundedSender`. When the
  sender is closed (receiver dropped), stop the stream. Wrap the
  `JoinHandle` in `TailHandle::new()`.

- **Error mapping**: Map your backend's errors to `BackendError` variants.
  The TUI uses these to show appropriate messages in the status bar.

## Step 3: Wire it up in the CLI

In `crates/oxo-cli/Cargo.toml`, add your crate as an optional dependency:

```toml
[features]
default = ["loki", "mybackend"]
mybackend = ["dep:oxo-mybackend"]

[dependencies]
oxo-mybackend = { workspace = true, optional = true }
```

In `crates/oxo-cli/src/main.rs`, add a match arm in `create_backend()`:

```rust
fn create_backend(config: &AppConfig) -> Result<Box<dyn LogBackend>> {
    match config.backend.as_str() {
        #[cfg(feature = "loki")]
        "loki" => { ... }

        #[cfg(feature = "mybackend")]
        "mybackend" => {
            let backend = oxo_mybackend::MyBackend::from_config(&config.connection)?;
            Ok(Box::new(backend))
        }

        other => anyhow::bail!("unknown backend: {other}"),
    }
}
```

## Step 4: Test it

```sh
# Unit tests
cargo test -p oxo-mybackend

# Manual testing
cargo run -- --backend mybackend --url http://localhost:9200
```

Consider using `wiremock` for HTTP mock testing, similar to `oxo-loki`.

## That's it!

The TUI will automatically work with your backend since it only interacts
through the `LogBackend` trait. No UI changes needed.
