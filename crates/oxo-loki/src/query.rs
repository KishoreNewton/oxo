//! LogQL query helpers.
//!
//! Provides utility functions for working with Loki's LogQL query language.
//! In the MVP this is mostly a pass-through, but it validates basic syntax
//! and offers helpers for common query patterns.

/// Validate that a LogQL query string has balanced braces.
///
/// This is a cheap client-side check to catch obvious mistakes before
/// sending the query to Loki. It does **not** fully parse LogQL.
///
/// # Examples
///
/// ```
/// use oxo_loki::query::validate_logql;
///
/// assert!(validate_logql(r#"{job="api"} |= "error""#));
/// assert!(!validate_logql(r#"{job="api""#)); // missing closing brace
/// ```
pub fn validate_logql(query: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in query.chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Build a simple stream selector from label matchers.
///
/// # Examples
///
/// ```
/// use oxo_loki::query::stream_selector;
///
/// let labels = vec![("job", "api"), ("namespace", "prod")];
/// assert_eq!(stream_selector(&labels), r#"{job="api", namespace="prod"}"#);
/// ```
pub fn stream_selector(labels: &[(&str, &str)]) -> String {
    let matchers: Vec<String> = labels
        .iter()
        .map(|(k, v)| format!(r#"{k}="{v}""#))
        .collect();
    format!("{{{}}}", matchers.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_balanced() {
        assert!(validate_logql(r#"{job="api"}"#));
        assert!(validate_logql(r#"{job="api"} |= "error""#));
        assert!(validate_logql(r#"{job="api"} | json | level="error""#));
    }

    #[test]
    fn test_validate_unbalanced() {
        assert!(!validate_logql(r#"{job="api""#));
        assert!(!validate_logql(r#"job="api"}"#));
        assert!(!validate_logql(r#"{{}"#));
    }

    #[test]
    fn test_validate_empty() {
        assert!(validate_logql(""));
    }

    #[test]
    fn test_stream_selector() {
        let labels = vec![("job", "api")];
        assert_eq!(stream_selector(&labels), r#"{job="api"}"#);

        let labels = vec![("job", "api"), ("env", "prod")];
        assert_eq!(stream_selector(&labels), r#"{job="api", env="prod"}"#);
    }

    #[test]
    fn test_stream_selector_empty() {
        let labels: Vec<(&str, &str)> = vec![];
        assert_eq!(stream_selector(&labels), "{}");
    }
}
