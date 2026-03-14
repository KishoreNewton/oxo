//! # oxo-demo
//!
//! A demo backend that generates fake log entries for testing the oxo TUI
//! without any real infrastructure. Useful for development, demos, and
//! getting a feel for the interface.
//!
//! ## Usage
//!
//! ```sh
//! oxo --backend demo
//! ```

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use rand::Rng;
use tokio::sync::mpsc;

use oxo_core::backend::{LogBackend, LogEntry, TailHandle};
use oxo_core::config::ConnectionConfig;
use oxo_core::error::BackendError;
use oxo_core::query::TimeRange;

/// Demo backend that produces synthetic log entries.
pub struct DemoBackend;

/// Sample plain-text log messages used by the demo backend.
const MESSAGES: &[&str] = &[
    "GET /api/v1/users 200 12ms",
    "POST /api/v1/orders 201 45ms",
    "GET /api/v1/health 200 1ms",
    "GET /api/v1/products?page=2 200 89ms",
    "POST /api/v1/auth/login 200 120ms",
    "DELETE /api/v1/sessions/abc123 204 8ms",
    "GET /api/v1/users/42/orders 200 230ms",
    "POST /api/v1/webhooks 202 15ms",
    "connection pool exhausted, waiting for available connection",
    "database query took 1.2s, slow query threshold exceeded",
    "cache miss for key user:42:profile, fetching from database",
    "rate limit exceeded for IP 192.168.1.100, returning 429",
    "failed to connect to payment gateway: connection timeout after 30s",
    "TLS handshake failed: certificate expired",
    "retrying request to upstream service (attempt 3/5)",
    "successfully processed batch of 1500 events in 2.3s",
    "websocket connection closed by client: code=1000",
    "new deployment detected: version=v2.4.1, rolling update started",
    "health check passed: cpu=23%, memory=67%, disk=45%",
    "message queue consumer lag: 4200 messages behind",
];

/// Sample JSON-formatted log messages (≈30 % of generated entries).
const JSON_MESSAGES: &[&str] = &[
    r#"{"msg":"Connection established","client_ip":"10.0.1.42","latency_ms":23,"trace_id":"abc123"}"#,
    r#"{"msg":"Request completed","method":"GET","path":"/api/v1/users","status":200,"duration_ms":12}"#,
    r#"{"msg":"Authentication failed","reason":"invalid_token","user_id":"u-9918","attempt":3}"#,
    r#"{"msg":"Cache miss","key":"product:555:detail","backend":"redis","fallback":"db"}"#,
    r#"{"msg":"Payment processed","amount":99.95,"currency":"USD","order_id":"ord-77231","provider":"stripe"}"#,
    r#"{"msg":"Slow query detected","query":"SELECT * FROM events","duration_ms":1340,"rows_examined":84000}"#,
    r#"{"msg":"Rate limit triggered","ip":"203.0.113.77","limit":100,"window_s":60,"remaining":0}"#,
    r#"{"msg":"Deployment started","version":"v2.4.1","strategy":"rolling","replicas":3,"initiator":"ci-bot"}"#,
    r#"{"msg":"Health check","status":"ok","cpu_pct":23,"mem_pct":67,"disk_pct":45}"#,
    r#"{"msg":"WebSocket disconnected","code":1001,"reason":"going away","session_id":"ws-4412","duration_s":187}"#,
    r#"{"msg":"Batch job finished","job":"invoice-export","records":1500,"elapsed_s":2.3,"errors":0}"#,
    r#"{"msg":"TLS error","error":"certificate expired","host":"payments.internal","expiry":"2024-01-01"}"#,
];

/// Sample services for label variety.
const SERVICES: &[&str] = &[
    "api-gateway",
    "auth-service",
    "order-service",
    "user-service",
    "payment-service",
];

/// Sample namespaces.
const NAMESPACES: &[&str] = &["prod", "staging"];

/// Log levels with weighted distribution (more info than errors).
const LEVELS: &[(&str, u32)] = &[
    ("debug", 10),
    ("info", 50),
    ("warn", 25),
    ("error", 12),
    ("fatal", 3),
];

/// Pick a random log level using weighted distribution.
fn random_level(rng: &mut impl Rng) -> &'static str {
    let total: u32 = LEVELS.iter().map(|(_, w)| w).sum();
    let mut roll: u32 = rng.random_range(0..total);
    for (level, weight) in LEVELS {
        if roll < *weight {
            return level;
        }
        roll -= weight;
    }
    "info"
}

/// Generate a single random log entry.
///
/// Roughly 30 % of entries use a JSON-formatted message so users can see
/// structured-field parsing in action within the detail panel.
fn random_entry(rng: &mut impl Rng) -> LogEntry {
    let use_json = rng.random_range(0u32..10) < 3; // 30 % probability
    let message = if use_json {
        JSON_MESSAGES[rng.random_range(0..JSON_MESSAGES.len())]
    } else {
        MESSAGES[rng.random_range(0..MESSAGES.len())]
    };

    let service = SERVICES[rng.random_range(0..SERVICES.len())];
    let namespace = NAMESPACES[rng.random_range(0..NAMESPACES.len())];
    let level = random_level(rng);

    let mut labels = BTreeMap::new();
    labels.insert("service".to_string(), service.to_string());
    labels.insert("namespace".to_string(), namespace.to_string());
    labels.insert("level".to_string(), level.to_string());

    LogEntry {
        timestamp: Utc::now(),
        labels,
        line: message.to_string(),
        raw: None,
    }
}

#[async_trait]
impl LogBackend for DemoBackend {
    fn name(&self) -> &str {
        "Demo"
    }

    async fn query(
        &self,
        _query: &str,
        _range: TimeRange,
        limit: usize,
    ) -> Result<Vec<LogEntry>, BackendError> {
        let mut rng = rand::rng();
        let entries: Vec<LogEntry> = (0..limit.min(100))
            .map(|_| random_entry(&mut rng))
            .collect();
        Ok(entries)
    }

    async fn tail(
        &self,
        _query: &str,
        tx: mpsc::UnboundedSender<LogEntry>,
    ) -> Result<TailHandle, BackendError> {
        let handle = tokio::spawn(async move {
            loop {
                // Generate 1-5 entries per batch at random intervals.
                // Scope the rng so it doesn't live across the await point
                // (rand::rng() returns a non-Send type).
                let (batch, delay_ms) = {
                    let mut rng = rand::rng();
                    let batch_size = rng.random_range(1..=5);
                    let batch: Vec<LogEntry> =
                        (0..batch_size).map(|_| random_entry(&mut rng)).collect();
                    let delay = rng.random_range(50..=500u64);
                    (batch, delay)
                };

                for entry in batch {
                    if tx.send(entry).is_err() {
                        return; // Receiver dropped, stop.
                    }
                }

                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        });

        Ok(TailHandle::new(handle))
    }

    async fn labels(&self) -> Result<Vec<String>, BackendError> {
        Ok(vec![
            "service".to_string(),
            "namespace".to_string(),
            "level".to_string(),
        ])
    }

    async fn label_values(&self, label: &str) -> Result<Vec<String>, BackendError> {
        let values = match label {
            "service" => SERVICES.iter().map(|s| s.to_string()).collect(),
            "namespace" => NAMESPACES.iter().map(|s| s.to_string()).collect(),
            "level" => LEVELS.iter().map(|(l, _)| l.to_string()).collect(),
            _ => vec![],
        };
        Ok(values)
    }

    async fn health(&self) -> Result<(), BackendError> {
        Ok(()) // Always healthy.
    }

    fn from_config(_config: &ConnectionConfig) -> Result<Self, BackendError> {
        Ok(DemoBackend)
    }
}
