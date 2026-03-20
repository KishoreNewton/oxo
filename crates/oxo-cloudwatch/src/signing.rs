//! AWS Signature Version 4 signing implementation.
//!
//! Implements the [Signature Version 4 signing process](https://docs.aws.amazon.com/general/latest/gr/signature-version-4.html)
//! used by all AWS service APIs. This module handles canonical request
//! construction, string-to-sign derivation, signing key computation, and
//! final `Authorization` header assembly.
//!
//! Only HMAC-SHA256 signing with SHA-256 payload hashing is supported, which
//! covers all CloudWatch Logs API operations.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// AWS credentials used for request signing.
#[derive(Debug, Clone)]
pub struct AwsCredentials {
    /// AWS access key ID.
    pub access_key: String,
    /// AWS secret access key.
    pub secret_key: String,
    /// Optional session token for temporary credentials (STS).
    pub session_token: Option<String>,
}

/// Signed header set returned by [`sign_request`].
///
/// Contains all headers that must be attached to the outgoing HTTP request,
/// including the `Authorization` header carrying the SigV4 signature.
#[derive(Debug)]
pub struct SignedHeaders {
    /// `Authorization` header value.
    pub authorization: String,
    /// `X-Amz-Date` header value (ISO 8601 basic format).
    pub amz_date: String,
    /// `X-Amz-Security-Token` header value, present only when using
    /// temporary credentials.
    pub security_token: Option<String>,
}

/// Sign an AWS API request using Signature Version 4.
///
/// # Arguments
///
/// * `method` — HTTP method (e.g. `"POST"`).
/// * `url` — The full request URL.
/// * `headers` — Sorted slice of `(header_name, header_value)` pairs that
///   will be included in the canonical request. Must include `host` and
///   `content-type` at minimum.
/// * `body` — The raw request body bytes.
/// * `region` — AWS region (e.g. `"us-east-1"`).
/// * `service` — AWS service name (e.g. `"logs"`).
/// * `credentials` — AWS access key, secret key, and optional session token.
/// * `timestamp` — The request timestamp. This determines the date scope
///   used in the signing key derivation.
///
/// # Returns
///
/// A [`SignedHeaders`] struct containing the `Authorization`, `X-Amz-Date`,
/// and optionally `X-Amz-Security-Token` headers to attach to the request.
#[allow(clippy::too_many_arguments)]
pub fn sign_request(
    method: &str,
    url: &url::Url,
    headers: &[(&str, &str)],
    body: &[u8],
    region: &str,
    service: &str,
    credentials: &AwsCredentials,
    timestamp: DateTime<Utc>,
) -> SignedHeaders {
    let amz_date = timestamp.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = timestamp.format("%Y%m%d").to_string();

    // ── Step 1: Canonical request ───────────────────────────────────

    let canonical_uri = if url.path().is_empty() {
        "/".to_string()
    } else {
        url.path().to_string()
    };

    let canonical_querystring = url.query().unwrap_or("");

    // Build sorted signed headers and their values.
    // We include the amz-date header in the canonical headers.
    let mut canonical_headers_list: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.trim().to_string()))
        .collect();
    canonical_headers_list.push(("x-amz-date".to_string(), amz_date.clone()));

    if let Some(ref token) = credentials.session_token {
        canonical_headers_list.push(("x-amz-security-token".to_string(), token.clone()));
    }

    canonical_headers_list.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = canonical_headers_list
        .iter()
        .map(|(k, v)| format!("{k}:{v}\n"))
        .collect();

    let signed_headers: String = canonical_headers_list
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let payload_hash = hex::encode(Sha256::digest(body));

    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_querystring}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );

    // ── Step 2: String to sign ──────────────────────────────────────

    let credential_scope = format!("{date_stamp}/{region}/{service}/aws4_request");
    let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));

    let string_to_sign =
        format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{canonical_request_hash}");

    // ── Step 3: Signing key ─────────────────────────────────────────

    let signing_key = derive_signing_key(&credentials.secret_key, &date_stamp, region, service);

    // ── Step 4: Signature ───────────────────────────────────────────

    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    // ── Step 5: Authorization header ────────────────────────────────

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        credentials.access_key, credential_scope, signed_headers, signature,
    );

    SignedHeaders {
        authorization,
        amz_date,
        security_token: credentials.session_token.clone(),
    }
}

/// Derive the SigV4 signing key.
///
/// ```text
/// kDate    = HMAC-SHA256("AWS4" + secret_key, date_stamp)
/// kRegion  = HMAC-SHA256(kDate,    region)
/// kService = HMAC-SHA256(kRegion,  service)
/// kSigning = HMAC-SHA256(kService, "aws4_request")
/// ```
fn derive_signing_key(secret_key: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_secret = format!("AWS4{secret_key}");
    let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

/// Compute HMAC-SHA256.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts keys of any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Test vector based on the AWS Signature V4 test suite.
    /// See: <https://docs.aws.amazon.com/general/latest/gr/sigv4-calculate-signature.html>
    #[test]
    fn signing_key_derivation() {
        let key = derive_signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
        );
        let expected = "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9";
        assert_eq!(hex::encode(&key), expected);
    }
}
