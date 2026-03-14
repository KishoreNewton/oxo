//! # oxo-alert
//!
//! Alert engine for the **oxo** observability TUI.
//!
//! This crate provides a rule-based alerting system that evaluates incoming
//! [`oxo_core::LogEntry`] values against user-defined rules and fires actions
//! (SMTP email, webhooks, desktop notifications) when conditions are met.
//!
//! # Architecture
//!
//! - [`config`] — TOML-deserializable alert configuration types.
//! - [`matcher`] — Compiles alert conditions into efficient matchers.
//! - [`state`] — Per-rule mutable state (cooldowns, rate windows).
//! - [`action`] — Async action executors (email, webhook, desktop).
//! - [`engine`] — The main [`AlertEngine`](engine::AlertEngine) that ties
//!   everything together.

pub mod action;
pub mod config;
pub mod engine;
pub mod matcher;
pub mod state;
