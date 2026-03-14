//! Structured log parsing utilities.
//!
//! Provides auto-detection and parsing of structured data embedded in log
//! lines, supporting JSON objects and `key=value` log formats (e.g. logfmt).

use std::collections::BTreeMap;

use serde_json::Value;

/// Represents structured data extracted from a log line.
#[derive(Debug, Clone)]
pub enum StructuredData {
    /// The log line was valid JSON.
    Json(serde_json::Map<String, Value>),
    /// The log line contained key=value pairs.
    KeyValue(BTreeMap<String, String>),
}

impl StructuredData {
    /// Try to parse structured data from a log line.
    ///
    /// Tries JSON first, then key=value (logfmt) format. Returns `None` if
    /// the line does not look structured.
    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Try JSON first.
        if let Some(json) = Self::try_json(trimmed) {
            return Some(json);
        }

        // Fall back to key=value parsing.
        Self::try_key_value(trimmed)
    }

    /// Return all fields as `(key, display_value)` pairs, suitable for UI
    /// rendering.
    pub fn fields(&self) -> Vec<(String, String)> {
        match self {
            StructuredData::Json(map) => map
                .iter()
                .map(|(k, v)| {
                    let display = match v {
                        Value::String(s) => s.clone(),
                        Value::Null => "null".to_string(),
                        other => other.to_string(),
                    };
                    (k.clone(), display)
                })
                .collect(),
            StructuredData::KeyValue(map) => {
                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }
        }
    }

    /// Get a specific field value by key, if present.
    pub fn get(&self, key: &str) -> Option<String> {
        match self {
            StructuredData::Json(map) => map.get(key).map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Null => "null".to_string(),
                other => other.to_string(),
            }),
            StructuredData::KeyValue(map) => map.get(key).cloned(),
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn try_json(s: &str) -> Option<Self> {
        // Quick syntactic gate: must start with '{'.
        if !s.starts_with('{') {
            return None;
        }
        match serde_json::from_str::<Value>(s) {
            Ok(Value::Object(map)) if !map.is_empty() => Some(StructuredData::Json(map)),
            _ => None,
        }
    }

    fn try_key_value(s: &str) -> Option<Self> {
        let pairs = parse_key_value(s);
        if pairs.len() >= 2 {
            Some(StructuredData::KeyValue(pairs.into_iter().collect()))
        } else {
            None
        }
    }
}

/// Parse a string of `key=value` pairs into a vec of `(key, value)` tuples.
///
/// Supports:
/// - `key=value` (unquoted, no spaces in value)
/// - `key="quoted value"` (double-quoted, respects `\"` escapes)
/// - `key='quoted value'` (single-quoted)
fn parse_key_value(s: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace between pairs.
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Read the key: everything up to '=' or whitespace.
        let key_start = i;
        while i < len && chars[i] != '=' && !chars[i].is_whitespace() {
            i += 1;
        }
        let key: String = chars[key_start..i].iter().collect();

        if key.is_empty() {
            // Shouldn't happen, but guard against infinite loop.
            i += 1;
            continue;
        }

        if i >= len || chars[i] != '=' {
            // No '=' found → not a key=value token; skip to next whitespace.
            while i < len && !chars[i].is_whitespace() {
                i += 1;
            }
            continue;
        }

        // Consume the '='.
        i += 1;

        if i >= len {
            // key= with nothing after it → empty value.
            pairs.push((key, String::new()));
            break;
        }

        // Read the value.
        let value = if chars[i] == '"' {
            // Double-quoted value.
            i += 1; // skip opening quote
            let mut v = String::new();
            while i < len && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1; // skip backslash
                    v.push(chars[i]);
                } else {
                    v.push(chars[i]);
                }
                i += 1;
            }
            if i < len {
                i += 1; // skip closing quote
            }
            v
        } else if chars[i] == '\'' {
            // Single-quoted value.
            i += 1; // skip opening quote
            let mut v = String::new();
            while i < len && chars[i] != '\'' {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                    v.push(chars[i]);
                } else {
                    v.push(chars[i]);
                }
                i += 1;
            }
            if i < len {
                i += 1; // skip closing quote
            }
            v
        } else {
            // Unquoted value: read until whitespace.
            let val_start = i;
            while i < len && !chars[i].is_whitespace() {
                i += 1;
            }
            chars[val_start..i].iter().collect()
        };

        pairs.push((key, value));
    }

    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_detection() {
        let line = r#"{"msg":"hello","level":"info","code":200}"#;
        let sd = StructuredData::parse(line).expect("should parse");
        assert!(matches!(sd, StructuredData::Json(_)));
        assert_eq!(sd.get("msg").as_deref(), Some("hello"));
        assert_eq!(sd.get("code").as_deref(), Some("200"));
    }

    #[test]
    fn test_json_non_object_rejected() {
        assert!(StructuredData::parse("[1,2,3]").is_none());
        assert!(StructuredData::parse("\"just a string\"").is_none());
        assert!(StructuredData::parse("42").is_none());
    }

    #[test]
    fn test_key_value_basic() {
        let line = r#"level=error msg="connection failed" duration=42ms"#;
        let sd = StructuredData::parse(line).expect("should parse");
        assert!(matches!(sd, StructuredData::KeyValue(_)));
        assert_eq!(sd.get("level").as_deref(), Some("error"));
        assert_eq!(sd.get("msg").as_deref(), Some("connection failed"));
        assert_eq!(sd.get("duration").as_deref(), Some("42ms"));
    }

    #[test]
    fn test_key_value_single_quotes() {
        let line = "level=warn msg='disk space low' host=web-01";
        let sd = StructuredData::parse(line).expect("should parse");
        assert_eq!(sd.get("msg").as_deref(), Some("disk space low"));
    }

    #[test]
    fn test_key_value_requires_two_pairs() {
        // Only one pair → should not be detected as structured.
        assert!(StructuredData::parse("level=error").is_none());
    }

    #[test]
    fn test_fields_order() {
        let line = r#"level=info msg="ok" svc=api"#;
        let sd = StructuredData::parse(line).unwrap();
        let fields = sd.fields();
        // BTreeMap gives alphabetical order.
        assert_eq!(fields[0].0, "level");
        assert_eq!(fields[1].0, "msg");
        assert_eq!(fields[2].0, "svc");
    }

    #[test]
    fn test_plain_line_not_detected() {
        assert!(StructuredData::parse("GET /api/v1/health 200 1ms").is_none());
    }

    #[test]
    fn test_json_nested_value_display() {
        let line = r#"{"service":"api","meta":{"region":"us-east-1"}}"#;
        let sd = StructuredData::parse(line).unwrap();
        // Nested objects are represented via their JSON serialisation.
        let meta = sd.get("meta").unwrap();
        assert!(meta.contains("region"));
    }
}
