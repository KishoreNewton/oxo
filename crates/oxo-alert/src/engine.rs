//! The main alert engine.
//!
//! [`AlertEngine`] consumes [`LogEntry`] values from an unbounded channel,
//! evaluates them against all compiled rules, and spawns async tasks to
//! execute any actions that fire. Results are reported back to the TUI via
//! an [`AlertEvent`] channel.

use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use oxo_core::LogEntry;

use crate::action::{execute_action, AlertContext};
use crate::config::{AlertConfig, AlertRule};
use crate::matcher::CompiledRule;
use crate::state::RuleState;

/// Events emitted by the alert engine back to the TUI.
#[derive(Debug, Clone)]
pub enum AlertEvent {
    /// A rule has fired (condition matched and cooldown passed).
    Fired {
        /// Name of the rule that fired.
        rule_name: String,
        /// Human-readable summary.
        message: String,
    },
    /// The result of executing a single action for a fired rule.
    ActionResult {
        /// Name of the rule the action belongs to.
        rule_name: String,
        /// The type of action that was executed (e.g. "email", "webhook").
        action_type: String,
        /// Whether the action succeeded.
        success: bool,
        /// Error description, if any.
        error: Option<String>,
    },
}

/// The core alert engine.
///
/// Create one via [`AlertEngine::new`], then call [`AlertEngine::run`] to
/// start consuming log entries. The engine will evaluate each entry against
/// all compiled rules, respecting cooldowns, and spawn action tasks for any
/// rules that fire.
pub struct AlertEngine {
    config: AlertConfig,
    rules: Vec<CompiledRule>,
    states: Vec<RuleState>,
    event_tx: mpsc::UnboundedSender<AlertEvent>,
}

impl AlertEngine {
    /// Compile all rules from `config` and construct a new engine.
    ///
    /// Rules that fail to compile (e.g. invalid regex) are logged and skipped.
    pub fn new(config: AlertConfig, event_tx: mpsc::UnboundedSender<AlertEvent>) -> Self {
        let mut rules = Vec::with_capacity(config.rules.len());
        let mut states = Vec::with_capacity(config.rules.len());

        for alert_rule in &config.rules {
            let window_seconds = match &alert_rule.condition {
                crate::config::AlertCondition::RateThreshold { window_seconds, .. } => {
                    *window_seconds
                }
                _ => 0,
            };

            match CompiledRule::compile(
                alert_rule.name.clone(),
                &alert_rule.condition,
                alert_rule.labels.clone(),
            ) {
                Ok(compiled) => {
                    rules.push(compiled);
                    states.push(RuleState::new(window_seconds));
                }
                Err(e) => {
                    warn!(
                        rule = %alert_rule.name,
                        error = %e,
                        "failed to compile alert rule, skipping"
                    );
                }
            }
        }

        debug!(count = rules.len(), "compiled alert rules");

        Self {
            config,
            rules,
            states,
            event_tx,
        }
    }

    /// Run the engine, consuming entries from the channel until it closes.
    pub async fn run(mut self, mut entry_rx: mpsc::UnboundedReceiver<LogEntry>) {
        if !self.config.enabled {
            debug!("alert engine disabled, draining channel");
            // Still drain so the sender doesn't block / leak.
            while entry_rx.recv().await.is_some() {}
            return;
        }

        debug!("alert engine started");

        while let Some(entry) = entry_rx.recv().await {
            self.evaluate(&entry).await;
        }

        debug!("alert engine shutting down (channel closed)");
    }

    /// Evaluate a single log entry against all rules.
    async fn evaluate(&mut self, entry: &LogEntry) {
        for i in 0..self.rules.len() {
            let cooldown = self.effective_cooldown(i);

            if self.rules[i].matches(entry, &mut self.states[i])
                && self.states[i].can_fire(cooldown)
            {
                self.states[i].mark_fired();

                let rule_name = self.rules[i].name.clone();
                let message = build_message(&self.rules[i], entry);

                // Notify that the rule has fired.
                let _ = self.event_tx.send(AlertEvent::Fired {
                    rule_name: rule_name.clone(),
                    message: message.clone(),
                });

                // Find the original AlertRule config to get actions.
                // We maintain a parallel index, so config.rules[i] corresponds
                // to self.rules[i] *only* if all rules compiled successfully.
                // Since we skip failed compilations, we need to find the rule
                // by name instead.
                if let Some(alert_rule) = self.find_rule_config(&rule_name) {
                    let actions = alert_rule.actions.clone();
                    let smtp = self.config.smtp.clone();
                    let ctx = AlertContext {
                        rule_name: rule_name.clone(),
                        message,
                        line_preview: truncate(&entry.line, 200),
                        timestamp: entry.timestamp,
                        labels: entry.labels.clone(),
                    };

                    for action_cfg in actions {
                        let smtp_ref = smtp.clone();
                        let tx = self.event_tx.clone();
                        let ctx_rule_name = ctx.rule_name.clone();
                        let ctx_message = ctx.message.clone();
                        let ctx_line_preview = ctx.line_preview.clone();
                        let ctx_timestamp = ctx.timestamp;
                        let ctx_labels = ctx.labels.clone();

                        tokio::spawn(async move {
                            let action_ctx = AlertContext {
                                rule_name: ctx_rule_name.clone(),
                                message: ctx_message,
                                line_preview: ctx_line_preview,
                                timestamp: ctx_timestamp,
                                labels: ctx_labels,
                            };
                            let result =
                                execute_action(&action_cfg, smtp_ref.as_ref(), &action_ctx).await;
                            match result {
                                Ok(action_type) => {
                                    let _ = tx.send(AlertEvent::ActionResult {
                                        rule_name: ctx_rule_name,
                                        action_type,
                                        success: true,
                                        error: None,
                                    });
                                }
                                Err(e) => {
                                    let action_type = match &action_cfg {
                                        crate::config::AlertActionConfig::Email { .. } => "email",
                                        crate::config::AlertActionConfig::Webhook { .. } => {
                                            "webhook"
                                        }
                                        crate::config::AlertActionConfig::Desktop { .. } => {
                                            "desktop"
                                        }
                                    };
                                    let _ = tx.send(AlertEvent::ActionResult {
                                        rule_name: ctx_rule_name,
                                        action_type: action_type.to_string(),
                                        success: false,
                                        error: Some(e),
                                    });
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    /// Return the effective cooldown for rule at index `i`.
    fn effective_cooldown(&self, i: usize) -> Duration {
        // Find the matching config rule by name.
        let per_rule = self
            .find_rule_config(&self.rules[i].name)
            .and_then(|r| r.cooldown_seconds);
        let secs = per_rule.unwrap_or(self.config.cooldown_seconds);
        Duration::from_secs(secs)
    }

    /// Look up the original [`AlertRule`] config by name.
    fn find_rule_config(&self, name: &str) -> Option<&AlertRule> {
        self.config.rules.iter().find(|r| r.name == name)
    }
}

/// Build a human-readable alert message from a rule and entry.
fn build_message(rule: &CompiledRule, entry: &LogEntry) -> String {
    format!(
        "Rule '{}' triggered on: {}",
        rule.name,
        truncate(&entry.line, 120)
    )
}

/// Truncate a string to at most `max` bytes, appending "..." if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find a valid char boundary at or before `max`.
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Utc;
    use tokio::sync::mpsc;

    use oxo_core::LogEntry;

    use crate::config::{AlertCondition, AlertConfig, AlertRule};

    use super::*;

    fn make_entry(line: &str, labels: &[(&str, &str)]) -> LogEntry {
        let mut lmap = BTreeMap::new();
        for &(k, v) in labels {
            lmap.insert(k.to_string(), v.to_string());
        }
        LogEntry {
            timestamp: Utc::now(),
            labels: lmap,
            line: line.to_string(),
            raw: None,
        }
    }

    #[tokio::test]
    async fn engine_fires_on_pattern_match() {
        let config = AlertConfig {
            enabled: true,
            cooldown_seconds: 0, // no cooldown for test
            smtp: None,
            rules: vec![AlertRule {
                name: "test-rule".into(),
                condition: AlertCondition::PatternMatch {
                    pattern: "FATAL".into(),
                },
                // Desktop action would fail in CI, so we use an empty actions
                // list and just check that the Fired event is emitted.
                actions: vec![],
                cooldown_seconds: Some(0),
                labels: Default::default(),
            }],
        };

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (entry_tx, entry_rx) = mpsc::unbounded_channel();

        let engine = AlertEngine::new(config, event_tx);
        let handle = tokio::spawn(engine.run(entry_rx));

        // Send a matching entry.
        entry_tx
            .send(make_entry("2024-01-01 FATAL panic", &[]))
            .unwrap();

        // Send a non-matching entry.
        entry_tx
            .send(make_entry("2024-01-01 INFO ok", &[]))
            .unwrap();

        // Close the channel so the engine shuts down.
        drop(entry_tx);
        handle.await.unwrap();

        // We should receive exactly one Fired event.
        let evt = event_rx.recv().await.expect("should receive event");
        match evt {
            AlertEvent::Fired { rule_name, .. } => {
                assert_eq!(rule_name, "test-rule");
            }
            other => panic!("expected Fired event, got {:?}", other),
        }

        // No more events (the INFO line should not have matched).
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn engine_respects_cooldown() {
        let config = AlertConfig {
            enabled: true,
            cooldown_seconds: 3600, // 1 hour — will not elapse during test
            smtp: None,
            rules: vec![AlertRule {
                name: "cd-rule".into(),
                condition: AlertCondition::PatternMatch {
                    pattern: "ERR".into(),
                },
                actions: vec![],
                cooldown_seconds: None, // use global
                labels: Default::default(),
            }],
        };

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (entry_tx, entry_rx) = mpsc::unbounded_channel();

        let engine = AlertEngine::new(config, event_tx);
        let handle = tokio::spawn(engine.run(entry_rx));

        // Send two matching entries.
        entry_tx.send(make_entry("ERR 1", &[])).unwrap();
        entry_tx.send(make_entry("ERR 2", &[])).unwrap();

        drop(entry_tx);
        handle.await.unwrap();

        // Only the first should fire; the second is within cooldown.
        let evt = event_rx.recv().await.expect("should receive first event");
        assert!(matches!(evt, AlertEvent::Fired { .. }));
        assert!(event_rx.try_recv().is_err(), "second should be suppressed by cooldown");
    }

    #[tokio::test]
    async fn engine_disabled_drains_channel() {
        let config = AlertConfig {
            enabled: false,
            ..Default::default()
        };

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (entry_tx, entry_rx) = mpsc::unbounded_channel();

        let engine = AlertEngine::new(config, event_tx);
        let handle = tokio::spawn(engine.run(entry_rx));

        entry_tx.send(make_entry("anything", &[])).unwrap();
        drop(entry_tx);
        handle.await.unwrap();

        // No events should be emitted.
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let s = "a".repeat(50);
        let t = truncate(&s, 10);
        assert_eq!(t.len(), 13); // 10 + "..."
        assert!(t.ends_with("..."));
    }
}
