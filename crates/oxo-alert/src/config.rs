//! Alert configuration types deserializable from TOML.
//!
//! The top-level [`AlertConfig`] is typically embedded in the application's
//! main configuration file under an `[alerts]` section.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level alert configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
    /// Whether the alert engine is enabled at all.
    pub enabled: bool,
    /// The set of alert rules to evaluate.
    pub rules: Vec<AlertRule>,
    /// Global SMTP configuration shared by all email actions.
    pub smtp: Option<SmtpConfig>,
    /// Default cooldown in seconds between repeated firings of the same rule.
    pub cooldown_seconds: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rules: Vec::new(),
            smtp: None,
            cooldown_seconds: 300,
        }
    }
}

/// A single alert rule: a condition plus one or more actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// Human-readable name for this rule (used in notifications).
    pub name: String,
    /// The condition that must be satisfied for this rule to fire.
    pub condition: AlertCondition,
    /// Actions to execute when the rule fires.
    pub actions: Vec<AlertActionConfig>,
    /// Per-rule cooldown override (seconds). Falls back to the global default.
    #[serde(default)]
    pub cooldown_seconds: Option<u64>,
    /// Optional label filters — all specified labels must match the log entry.
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

/// The condition under which an alert rule fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertCondition {
    /// Fire when the log line matches a regex pattern.
    PatternMatch { pattern: String },
    /// Fire when the number of matching entries in a sliding window exceeds
    /// `count` within `window_seconds`.
    RateThreshold { count: u64, window_seconds: u64 },
    /// Fire when the log entry's level is at or above the given severity.
    LevelThreshold { level: String },
}

/// SMTP server configuration for email actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// SMTP server hostname.
    pub host: String,
    /// SMTP server port.
    pub port: u16,
    /// SMTP username for authentication.
    pub username: String,
    /// SMTP password for authentication.
    pub password: String,
    /// The "From" address for outgoing emails.
    pub from: String,
    /// Whether to use STARTTLS (default: true).
    #[serde(default = "default_true")]
    pub starttls: bool,
}

fn default_true() -> bool {
    true
}

/// Configuration for a single action to execute when a rule fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertActionConfig {
    /// Send an email via SMTP.
    Email {
        /// Recipient email addresses.
        to: Vec<String>,
        /// Optional subject template. Supports `{rule_name}`, `{line_preview}`,
        /// `{timestamp}`, `{level}` placeholders.
        subject_template: Option<String>,
    },
    /// Send an HTTP request to a webhook URL.
    Webhook {
        /// The webhook URL.
        url: String,
        /// HTTP method (default: POST).
        #[serde(default)]
        method: Option<String>,
        /// Extra headers to include in the request.
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    /// Show a desktop notification.
    Desktop {
        /// Optional notification title override.
        #[serde(default)]
        title: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_config() {
        let toml_str = r#"
enabled = true
cooldown_seconds = 60

[smtp]
host = "smtp.example.com"
port = 587
username = "alerts@example.com"
password = "hunter2"
from = "alerts@example.com"

[[rules]]
name = "error-spike"
[rules.condition]
type = "rate_threshold"
count = 50
window_seconds = 60
[[rules.actions]]
type = "email"
to = ["oncall@example.com"]
subject_template = "[oxo] {rule_name}: rate spike detected"
[rules.labels]
env = "production"

[[rules]]
name = "fatal-pattern"
cooldown_seconds = 600
[rules.condition]
type = "pattern_match"
pattern = "FATAL|panic|SIGSEGV"
[[rules.actions]]
type = "webhook"
url = "https://hooks.slack.com/services/T00/B00/xxx"
[rules.actions.headers]
Authorization = "Bearer tok"
[[rules.actions]]
type = "desktop"
"#;
        let cfg: AlertConfig = toml::from_str(toml_str).expect("should parse");
        assert!(cfg.enabled);
        assert_eq!(cfg.cooldown_seconds, 60);
        assert_eq!(cfg.rules.len(), 2);

        // First rule
        let r0 = &cfg.rules[0];
        assert_eq!(r0.name, "error-spike");
        assert!(matches!(
            r0.condition,
            AlertCondition::RateThreshold {
                count: 50,
                window_seconds: 60
            }
        ));
        assert_eq!(r0.actions.len(), 1);
        assert_eq!(r0.labels.get("env").unwrap(), "production");

        // Second rule
        let r1 = &cfg.rules[1];
        assert_eq!(r1.name, "fatal-pattern");
        assert_eq!(r1.cooldown_seconds, Some(600));
        assert!(matches!(r1.condition, AlertCondition::PatternMatch { .. }));
        assert_eq!(r1.actions.len(), 2);

        // SMTP
        let smtp = cfg.smtp.as_ref().unwrap();
        assert_eq!(smtp.host, "smtp.example.com");
        assert_eq!(smtp.port, 587);
        assert!(smtp.starttls); // default
    }

    #[test]
    fn deserialize_default_config() {
        let cfg: AlertConfig = toml::from_str("").expect("empty should parse");
        assert!(!cfg.enabled);
        assert!(cfg.rules.is_empty());
        assert!(cfg.smtp.is_none());
        assert_eq!(cfg.cooldown_seconds, 300);
    }

    #[test]
    fn deserialize_level_threshold() {
        let toml_str = r#"
[[rules]]
name = "warn-and-above"
[rules.condition]
type = "level_threshold"
level = "warn"
[[rules.actions]]
type = "desktop"
"#;
        let cfg: AlertConfig = toml::from_str(toml_str).expect("should parse");
        assert_eq!(cfg.rules.len(), 1);
        assert!(matches!(
            cfg.rules[0].condition,
            AlertCondition::LevelThreshold { .. }
        ));
    }
}
