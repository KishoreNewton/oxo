//! HTTP client for the Elasticsearch / OpenSearch API.
//!
//! Wraps [`reqwest::Client`] with Elasticsearch-specific URL construction,
//! authentication, and response parsing. Each public method maps to an
//! Elasticsearch REST API endpoint.
//!
//! See: <https://www.elastic.co/guide/en/elasticsearch/reference/current/rest-apis.html>

use chrono::{DateTime, Utc};
use reqwest::{Client, RequestBuilder};
use serde_json::{Value, json};
use url::Url;

use oxo_core::config::{AuthConfig, ConnectionConfig};
use oxo_core::error::BackendError;

use crate::response::{
    AggResponse, ClusterHealthResponse, EsHit, FieldCapsResponse, SearchResponse,
};

/// HTTP client for communicating with an Elasticsearch or OpenSearch cluster.
///
/// Handles base URL construction, index pattern targeting, authentication
/// headers, and response deserialization.
#[derive(Debug, Clone)]
pub struct ElasticsearchClient {
    /// The underlying HTTP client (connection-pooled).
    http: Client,
    /// Base URL of the Elasticsearch cluster (e.g. `http://localhost:9200`).
    base_url: Url,
    /// Authentication configuration.
    auth: Option<AuthConfig>,
    /// Default index pattern to query (e.g. `"logs-*"`, `"*"`).
    index: String,
}

impl ElasticsearchClient {
    /// Create a new Elasticsearch client from connection configuration.
    ///
    /// Reads `extra.get("index")` from the config to determine the default
    /// index pattern. Falls back to `"*"` if unset.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::Connection`] if the URL is malformed or the
    /// HTTP client cannot be constructed.
    pub fn new(config: &ConnectionConfig) -> Result<Self, BackendError> {
        let base_url = Url::parse(&config.url).map_err(|e| {
            BackendError::Connection(format!("invalid Elasticsearch URL: {e}"))
        })?;

        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            // Elasticsearch may use self-signed certs in dev environments.
            .danger_accept_invalid_certs(
                config
                    .extra
                    .get("insecure")
                    .is_some_and(|v| v == "true" || v == "1"),
            )
            .build()
            .map_err(|e| {
                BackendError::Connection(format!("failed to build HTTP client: {e}"))
            })?;

        let index = config
            .extra
            .get("index")
            .cloned()
            .unwrap_or_else(|| "*".to_string());

        Ok(Self {
            http,
            base_url,
            auth: config.auth.clone(),
            index,
        })
    }

    /// Return the default index pattern.
    pub fn index(&self) -> &str {
        &self.index
    }

    /// Return the base URL (used by the tail module).
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Return the auth config (used by the tail module).
    pub fn auth(&self) -> &Option<AuthConfig> {
        &self.auth
    }

    // ── Search ──────────────────────────────────────────────────────

    /// Execute a search query against an Elasticsearch index.
    ///
    /// Translates the caller's query string into an Elasticsearch
    /// `query_string` query combined with a `@timestamp` range filter.
    /// An empty or `"{}"` query is treated as `match_all`.
    ///
    /// Results are sorted by `@timestamp` descending and capped at `limit`.
    pub async fn search(
        &self,
        index: &str,
        query: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<EsHit>, BackendError> {
        let body = build_search_body(query, from, to, limit, None);
        let url = self.endpoint(&format!("/{}/_search", index))?;

        let resp = self
            .authenticated(self.http.post(url))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    BackendError::Timeout(std::time::Duration::from_secs(30))
                } else if e.is_connect() {
                    BackendError::Connection(format!("search request failed: {e}"))
                } else {
                    BackendError::Connection(format!("search request failed: {e}"))
                }
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            let body = resp.text().await.unwrap_or_default();
            return Err(BackendError::Auth(format!(
                "Elasticsearch returned {status}: {body}"
            )));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(BackendError::RateLimited { retry_after: None });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BackendError::Query(format!(
                "Elasticsearch returned {status}: {body}"
            )));
        }

        let search_resp: SearchResponse = resp.json().await.map_err(|e| {
            BackendError::Query(format!("failed to parse search response: {e}"))
        })?;

        Ok(search_resp.hits.hits)
    }

    // ── Field Capabilities ──────────────────────────────────────────

    /// Discover available field names across the given index pattern.
    ///
    /// Calls `GET /{index}/_field_caps?fields=*` and returns the list of
    /// field names, filtering out internal metadata fields (those starting
    /// with `_`).
    pub async fn field_caps(
        &self,
        index: &str,
    ) -> Result<Vec<String>, BackendError> {
        let url = self.endpoint(&format!("/{}/_field_caps", index))?;

        let resp = self
            .authenticated(self.http.get(url))
            .query(&[("fields", "*")])
            .send()
            .await
            .map_err(|e| {
                BackendError::Connection(format!("field_caps request failed: {e}"))
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BackendError::Query(format!(
                "Elasticsearch returned {status}: {body}"
            )));
        }

        let caps: FieldCapsResponse = resp.json().await.map_err(|e| {
            BackendError::Query(format!("failed to parse field_caps response: {e}"))
        })?;

        let mut fields: Vec<String> = caps
            .fields
            .into_keys()
            .filter(|name| !name.starts_with('_'))
            .collect();
        fields.sort();
        Ok(fields)
    }

    // ── Field Values (Terms Aggregation) ────────────────────────────

    /// Retrieve the top distinct values for a given field.
    ///
    /// Uses a terms aggregation to discover up to 100 unique values,
    /// which powers the label-value autocomplete in the TUI.
    pub async fn field_values(
        &self,
        index: &str,
        field: &str,
    ) -> Result<Vec<String>, BackendError> {
        let url = self.endpoint(&format!("/{}/_search", index))?;

        let body = json!({
            "size": 0,
            "aggs": {
                "values": {
                    "terms": {
                        "field": field,
                        "size": 100
                    }
                }
            }
        });

        let resp = self
            .authenticated(self.http.post(url))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                BackendError::Connection(format!("field_values request failed: {e}"))
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BackendError::Query(format!(
                "Elasticsearch returned {status}: {body}"
            )));
        }

        let agg_resp: AggResponse = resp.json().await.map_err(|e| {
            BackendError::Query(format!("failed to parse aggregation response: {e}"))
        })?;

        let values = agg_resp
            .aggregations
            .map(|aggs| {
                aggs.values
                    .buckets
                    .into_iter()
                    .map(|b| match b.key {
                        Value::String(s) => s,
                        other => other.to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(values)
    }

    // ── Cluster Health ──────────────────────────────────────────────

    /// Perform a cluster health check.
    ///
    /// Calls `GET /_cluster/health` and considers `green` or `yellow`
    /// status as healthy. A `red` status or connection failure is an error.
    pub async fn health(&self) -> Result<(), BackendError> {
        let url = self.endpoint("/_cluster/health")?;

        let resp = self
            .authenticated(self.http.get(url))
            .send()
            .await
            .map_err(|e| {
                BackendError::Connection(format!("health check failed: {e}"))
            })?;

        let status = resp.status();
        if !status.is_success() {
            return Err(BackendError::Connection(format!(
                "Elasticsearch not reachable: HTTP {status}"
            )));
        }

        let health: ClusterHealthResponse = resp.json().await.map_err(|e| {
            BackendError::Connection(format!("failed to parse health response: {e}"))
        })?;

        match health.status.as_str() {
            "green" | "yellow" => Ok(()),
            other => Err(BackendError::Connection(format!(
                "cluster health is {other}"
            ))),
        }
    }

    // ── Scroll Search (for tail polling) ────────────────────────────

    /// Execute a search with `search_after` for cursor-based pagination.
    ///
    /// This is used by the tail module to poll for new documents since the
    /// last known sort value. Results are sorted by `@timestamp` ascending
    /// so new entries appear in chronological order.
    ///
    /// # Arguments
    ///
    /// * `index` — Index pattern to search.
    /// * `query` — User query string (empty / `"{}"` → match_all).
    /// * `from` — Start of the time range.
    /// * `to` — End of the time range.
    /// * `search_after` — The sort values from the last seen hit (cursor).
    pub async fn scroll_search(
        &self,
        index: &str,
        query: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        search_after: Option<&[Value]>,
    ) -> Result<Vec<EsHit>, BackendError> {
        let mut body = build_search_body_asc(query, from, to, 500, search_after);

        // For tail we want ascending order to get oldest-first.
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "sort".to_string(),
                json!([{ "@timestamp": { "order": "asc" } }, { "_id": { "order": "asc" } }]),
            );
        }

        let url = self.endpoint(&format!("/{}/_search", index))?;

        let resp = self
            .authenticated(self.http.post(url))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                BackendError::Connection(format!("scroll search failed: {e}"))
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BackendError::Query(format!(
                "Elasticsearch returned {status}: {body}"
            )));
        }

        let search_resp: SearchResponse = resp.json().await.map_err(|e| {
            BackendError::Query(format!("failed to parse scroll response: {e}"))
        })?;

        Ok(search_resp.hits.hits)
    }

    // ── Private helpers ─────────────────────────────────────────────

    /// Build a full URL for an Elasticsearch API endpoint.
    fn endpoint(&self, path: &str) -> Result<Url, BackendError> {
        self.base_url.join(path).map_err(|e| {
            BackendError::Connection(format!(
                "failed to build URL for {path}: {e}"
            ))
        })
    }

    /// Apply authentication headers to a request.
    fn authenticated(&self, mut req: RequestBuilder) -> RequestBuilder {
        match &self.auth {
            Some(AuthConfig::Basic { username, password }) => {
                req = req.basic_auth(username, Some(password));
            }
            Some(AuthConfig::Bearer { token }) => {
                req = req.bearer_auth(token);
            }
            None => {}
        }
        req
    }
}

// ── Query body builders ─────────────────────────────────────────────

/// Build an Elasticsearch search request body (descending order).
///
/// If `query` is empty or `"{}"`, uses `match_all`. Otherwise wraps
/// the query string in a `query_string` query combined with a
/// `@timestamp` range filter.
fn build_search_body(
    query: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: usize,
    search_after: Option<&[Value]>,
) -> Value {
    let trimmed = query.trim();
    let is_empty = trimmed.is_empty() || trimmed == "{}";

    let query_clause: Value = if is_empty {
        json!({ "match_all": {} })
    } else {
        json!({ "query_string": { "query": trimmed } })
    };

    let from_iso = from.to_rfc3339();
    let to_iso = to.to_rfc3339();

    let range_clause = json!({
        "range": {
            "@timestamp": {
                "gte": from_iso,
                "lte": to_iso
            }
        }
    });

    let bool_query = json!({
        "bool": {
            "must": [query_clause, range_clause]
        }
    });

    let mut body = json!({
        "query": bool_query,
        "sort": [{ "@timestamp": { "order": "desc" } }],
        "size": limit
    });

    if let Some(sa) = search_after {
        body.as_object_mut()
            .unwrap()
            .insert("search_after".to_string(), Value::Array(sa.to_vec()));
    }

    body
}

/// Build an Elasticsearch search request body (ascending order, for tail).
fn build_search_body_asc(
    query: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: usize,
    search_after: Option<&[Value]>,
) -> Value {
    let trimmed = query.trim();
    let is_empty = trimmed.is_empty() || trimmed == "{}";

    let query_clause: Value = if is_empty {
        json!({ "match_all": {} })
    } else {
        json!({ "query_string": { "query": trimmed } })
    };

    let from_iso = from.to_rfc3339();
    let to_iso = to.to_rfc3339();

    let range_clause = json!({
        "range": {
            "@timestamp": {
                "gte": from_iso,
                "lte": to_iso
            }
        }
    });

    let bool_query = json!({
        "bool": {
            "must": [query_clause, range_clause]
        }
    });

    let mut body = json!({
        "query": bool_query,
        "sort": [
            { "@timestamp": { "order": "asc" } },
            { "_id": { "order": "asc" } }
        ],
        "size": limit
    });

    if let Some(sa) = search_after {
        body.as_object_mut()
            .unwrap()
            .insert("search_after".to_string(), Value::Array(sa.to_vec()));
    }

    body
}
