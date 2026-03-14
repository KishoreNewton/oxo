//! Action executors.
//!
//! Each alert action (email, webhook, desktop notification) is executed
//! asynchronously. The public entry point is [`execute_action`], which
//! dispatches to the appropriate handler based on the action configuration.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use tracing::{debug, error};

use crate::config::{AlertActionConfig, SmtpConfig};

/// Context passed to every action executor, describing the alert that fired.
pub struct AlertContext {
    /// Name of the rule that fired.
    pub rule_name: String,
    /// A human-readable summary message.
    pub message: String,
    /// A short preview of the log line that triggered the alert.
    pub line_preview: String,
    /// Timestamp of the triggering log entry.
    pub timestamp: DateTime<Utc>,
    /// Labels from the triggering log entry.
    pub labels: BTreeMap<String, String>,
}

/// Execute a single alert action.
///
/// Returns `Ok(action_type_name)` on success or `Err(description)` on failure.
pub async fn execute_action(
    action: &AlertActionConfig,
    smtp_config: Option<&SmtpConfig>,
    ctx: &AlertContext,
) -> Result<String, String> {
    match action {
        AlertActionConfig::Email {
            to,
            subject_template,
        } => {
            let smtp = smtp_config.ok_or_else(|| {
                "email action configured but no SMTP config provided".to_string()
            })?;
            let subject = match subject_template {
                Some(tpl) => render_template(tpl, ctx),
                None => format!("[oxo-alert] Rule '{}' fired", ctx.rule_name),
            };
            let body = format!(
                "Alert: {}\n\nRule: {}\nTime: {}\nLog line: {}\nLabels: {:?}\n",
                ctx.message, ctx.rule_name, ctx.timestamp, ctx.line_preview, ctx.labels,
            );
            send_email(smtp, to, subject, body).await?;
            Ok("email".to_string())
        }
        AlertActionConfig::Webhook {
            url,
            method,
            headers,
        } => {
            send_webhook(url, method.as_deref(), headers, ctx).await?;
            Ok("webhook".to_string())
        }
        AlertActionConfig::Desktop { title } => {
            send_desktop(title.as_deref(), ctx)?;
            Ok("desktop".to_string())
        }
    }
}

/// Perform template substitution on a string.
///
/// Supported placeholders: `{rule_name}`, `{line_preview}`, `{timestamp}`,
/// `{level}`.
fn render_template(template: &str, ctx: &AlertContext) -> String {
    let level = ctx
        .labels
        .get("level")
        .or_else(|| ctx.labels.get("severity"))
        .cloned()
        .unwrap_or_default();
    template
        .replace("{rule_name}", &ctx.rule_name)
        .replace("{line_preview}", &ctx.line_preview)
        .replace("{timestamp}", &ctx.timestamp.to_rfc3339())
        .replace("{level}", &level)
}

// ---------------------------------------------------------------------------
// Email
// ---------------------------------------------------------------------------

async fn send_email(
    smtp: &SmtpConfig,
    to: &[String],
    subject: String,
    body: String,
) -> Result<(), String> {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let creds = Credentials::new(smtp.username.clone(), smtp.password.clone());

    let mailer = if smtp.starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)
            .map_err(|e| format!("SMTP relay error: {e}"))?
            .port(smtp.port)
            .credentials(creds)
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host)
            .port(smtp.port)
            .credentials(creds)
            .build()
    };

    for recipient in to {
        let email = Message::builder()
            .from(
                smtp.from
                    .parse()
                    .map_err(|e| format!("bad from address: {e}"))?,
            )
            .to(recipient
                .parse()
                .map_err(|e| format!("bad to address '{recipient}': {e}"))?)
            .subject(&subject)
            .body(body.clone())
            .map_err(|e| format!("build email: {e}"))?;

        mailer
            .send(email)
            .await
            .map_err(|e| format!("send to '{recipient}' failed: {e}"))?;

        debug!(to = %recipient, "alert email sent");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Webhook
// ---------------------------------------------------------------------------

async fn send_webhook(
    url: &str,
    method: Option<&str>,
    headers: &std::collections::HashMap<String, String>,
    ctx: &AlertContext,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    let method_str = method.unwrap_or("POST");
    let http_method: reqwest::Method = method_str
        .parse()
        .map_err(|_| format!("invalid HTTP method: {method_str}"))?;

    let payload = serde_json::json!({
        "rule_name": ctx.rule_name,
        "message": ctx.message,
        "line_preview": ctx.line_preview,
        "timestamp": ctx.timestamp.to_rfc3339(),
        "labels": ctx.labels,
    });

    let mut req = client.request(http_method, url).json(&payload);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let resp = req.send().await.map_err(|e| format!("webhook request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".into());
        error!(url, %status, "webhook returned non-success");
        return Err(format!("webhook {url} returned {status}: {body}"));
    }

    debug!(url, "webhook delivered");
    Ok(())
}

// ---------------------------------------------------------------------------
// Desktop notification
// ---------------------------------------------------------------------------

fn send_desktop(title: Option<&str>, ctx: &AlertContext) -> Result<(), String> {
    let title = title.unwrap_or("oxo alert");
    let body = format!("{}: {}", ctx.rule_name, ctx.message);

    notify_rust::Notification::new()
        .summary(title)
        .body(&body)
        .show()
        .map_err(|e| format!("desktop notification failed: {e}"))?;

    debug!("desktop notification shown");
    Ok(())
}
