//! HTTP client for the Loki API.
//!
//! Wraps [`reqwest::Client`] with Loki-specific URL construction,
//! authentication, and response parsing. Each public method maps 1:1
//! to a Loki HTTP API endpoint.
//!
//! See: <https://grafana.com/docs/loki/latest/reference/loki-http-api/>

use reqwest::{Client, RequestBuilder};
use url::Url;

use oxo_core::config::{AuthConfig, ConnectionConfig};
use oxo_core::error::BackendError;

use crate::response::{LabelValuesResponse, LabelsResponse, LokiResponse, LokiStream};

/// HTTP client for communicating with a Loki instance.
///
/// Handles base URL construction, authentication headers, and response
/// deserialization. All methods return deserialized Loki types from
/// [`crate::response`].
#[derive(Debug, Clone)]
pub struct LokiClient {
    /// The underlying HTTP client (connection-pooled).
    http: Client,
    /// Base URL of the Loki instance (e.g. `http://localhost:3100`).
    base_url: Url,
    /// Authentication configuration.
    auth: Option<AuthConfig>,
    /// Optional Loki tenant ID for multi-tenant deployments.
    org_id: Option<String>,
}

impl LokiClient {
    /// Create a new Loki client from connection configuration.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::Connection`] if the URL is malformed.
    pub fn new(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let base_url = Url::parse(&config.url)
            .map_err(|e| BackendError::Connection(format!("invalid Loki URL: {e}")))?;

        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| BackendError::Connection(format!("failed to build HTTP client: {e}")))?;

        let org_id = config.extra.get("org_id").cloned();

        Ok(Self {
            http,
            base_url,
            auth: config.auth.clone(),
            org_id,
        })
    }

    /// Return the base URL (used by the tail module to construct WebSocket URLs).
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Return the auth config (used by the tail module for WebSocket auth).
    pub fn auth(&self) -> &Option<AuthConfig> {
        &self.auth
    }

    /// Return the org ID (used by the tail module for multi-tenant headers).
    pub fn org_id(&self) -> Option<&str> {
        self.org_id.as_deref()
    }

    /// Query logs over a time range.
    ///
    /// Calls `GET /loki/api/v1/query_range`.
    ///
    /// # Arguments
    ///
    /// * `logql` — A LogQL query string (e.g. `{job="api"} |= "error"`).
    /// * `start` — Start of the time range as a nanosecond Unix timestamp.
    /// * `end` — End of the time range as a nanosecond Unix timestamp.
    /// * `limit` — Maximum number of entries to return.
    pub async fn query_range(
        &self,
        logql: &str,
        start: i64,
        end: i64,
        limit: usize,
    ) -> Result<Vec<LokiStream>, BackendError> {
        let url = self.endpoint("/loki/api/v1/query_range")?;

        let resp = self
            .authenticated(self.http.get(url))
            .query(&[
                ("query", logql),
                ("start", &start.to_string()),
                ("end", &end.to_string()),
                ("limit", &limit.to_string()),
                ("direction", "backward"),
            ])
            .send()
            .await
            .map_err(|e| BackendError::Connection(format!("query_range request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BackendError::Query(format!(
                "Loki returned {status}: {body}"
            )));
        }

        let loki_resp: LokiResponse = resp
            .json()
            .await
            .map_err(|e| BackendError::Query(format!("failed to parse Loki response: {e}")))?;

        Ok(loki_resp.data.result)
    }

    /// Fetch all known label names.
    ///
    /// Calls `GET /loki/api/v1/labels`.
    pub async fn labels(&self) -> Result<Vec<String>, BackendError> {
        let url = self.endpoint("/loki/api/v1/labels")?;

        let resp = self
            .authenticated(self.http.get(url))
            .send()
            .await
            .map_err(|e| BackendError::Connection(format!("labels request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BackendError::Query(format!(
                "Loki returned {status}: {body}"
            )));
        }

        let labels_resp: LabelsResponse = resp
            .json()
            .await
            .map_err(|e| BackendError::Query(format!("failed to parse labels response: {e}")))?;

        Ok(labels_resp.data)
    }

    /// Fetch known values for a given label.
    ///
    /// Calls `GET /loki/api/v1/label/{name}/values`.
    pub async fn label_values(&self, name: &str) -> Result<Vec<String>, BackendError> {
        let path = format!("/loki/api/v1/label/{name}/values");
        let url = self.endpoint(&path)?;

        let resp = self
            .authenticated(self.http.get(url))
            .send()
            .await
            .map_err(|e| BackendError::Connection(format!("label_values request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BackendError::Query(format!(
                "Loki returned {status}: {body}"
            )));
        }

        let values_resp: LabelValuesResponse = resp.json().await.map_err(|e| {
            BackendError::Query(format!("failed to parse label_values response: {e}"))
        })?;

        Ok(values_resp.data)
    }

    /// Health check — calls `GET /ready`.
    pub async fn health(&self) -> Result<(), BackendError> {
        let url = self.endpoint("/ready")?;

        let resp = self
            .authenticated(self.http.get(url))
            .send()
            .await
            .map_err(|e| BackendError::Connection(format!("health check failed: {e}")))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(BackendError::Connection(format!(
                "Loki not ready: HTTP {}",
                resp.status()
            )))
        }
    }

    // ── Private helpers ──────────────────────────────────────────────

    /// Build a full URL for a Loki API endpoint.
    fn endpoint(&self, path: &str) -> Result<Url, BackendError> {
        self.base_url
            .join(path)
            .map_err(|e| BackendError::Connection(format!("failed to build URL for {path}: {e}")))
    }

    /// Apply authentication and tenant headers to a request.
    fn authenticated(&self, mut req: RequestBuilder) -> RequestBuilder {
        // Auth header.
        match &self.auth {
            Some(AuthConfig::Basic { username, password }) => {
                req = req.basic_auth(username, Some(password));
            }
            Some(AuthConfig::Bearer { token }) => {
                req = req.bearer_auth(token);
            }
            None => {}
        }

        // Multi-tenant header.
        if let Some(org_id) = &self.org_id {
            req = req.header("X-Scope-OrgID", org_id);
        }

        req
    }
}
