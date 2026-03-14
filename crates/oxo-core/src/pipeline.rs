//! Client-side log processing pipeline.
//!
//! Provides a backend-agnostic pipeline engine inspired by LogQL's pipeline
//! stages. A [`Pipeline`] is parsed from the portion of a query that follows
//! the stream selector and consists of one or more [`PipelineStage`]s that
//! filter and transform [`LogEntry`] values.
//!
//! # Example
//!
//! ```
//! use oxo_core::pipeline::Pipeline;
//!
//! let input = r#"{job="api"} | json | level="error""#;
//! let (selector, pipeline) = Pipeline::parse(input).unwrap();
//! assert_eq!(selector, r#"{job="api"}"#);
//! assert!(!pipeline.is_empty());
//! ```

use std::collections::BTreeMap;

use regex::Regex;
use serde_json::Value;

use crate::backend::LogEntry;

// ---------------------------------------------------------------------------
// Stage types
// ---------------------------------------------------------------------------

/// A single processing stage in a pipeline.
#[derive(Debug, Clone)]
pub enum PipelineStage {
    /// Parse the log line as JSON and promote string fields to labels.
    Json,
    /// Parse `key=value` pairs (logfmt) and promote to labels.
    Logfmt,
    /// Extract named captures from a regex and promote to labels.
    Regex { pattern: String },
    /// Keep or drop entries based on a label comparison.
    LabelFilter {
        label: String,
        op: FilterOp,
        value: String,
    },
    /// Keep or drop entries based on line content.
    LineFilter {
        pattern: String,
        negate: bool,
        /// When true, `pattern` is always treated as a regex (`|~` / `!~`).
        /// When false, `pattern` is a literal substring (`|=` / `!=`).
        regex: bool,
    },
    /// Rewrite the log line using a `{{.label}}` template.
    LineFormat { template: String },
    /// Remove the listed labels from every entry.
    LabelDrop { labels: Vec<String> },
    /// Keep only the listed labels, removing all others.
    LabelKeep { labels: Vec<String> },
    /// Remove consecutive entries with identical log lines.
    Dedup,
    /// Recursively flatten nested JSON objects into dot-notation labels.
    Unpack,
    /// Marker for the metrics system — counts entries per time window.
    /// No-op in `apply`.
    Rate,
    /// Keep every Nth entry (1-indexed: keeps entries at index 0, N, 2N, ...).
    Sample { n: usize },
    /// Keep only the first N entries.
    Limit { n: usize },
}

/// Comparison operator for [`PipelineStage::LabelFilter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterOp {
    /// Exact equality (`=`).
    Eq,
    /// Not equal (`!=`).
    Neq,
    /// Regex match (`=~`).
    Re,
    /// Negated regex match (`!~`).
    Nre,
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// An ordered sequence of pipeline stages.
#[derive(Debug, Clone)]
pub struct Pipeline {
    stages: Vec<PipelineStage>,
}

impl Pipeline {
    /// Parse a query string into a stream selector and a pipeline.
    ///
    /// The stream selector is everything up to the first `|` that is not
    /// inside braces or quotes. The remainder is split on `|` and each
    /// token is parsed into a [`PipelineStage`].
    ///
    /// Returns `(stream_selector, pipeline)`.
    pub fn parse(input: &str) -> Result<(String, Pipeline), String> {
        let input = input.trim();
        if input.is_empty() {
            return Ok((String::new(), Pipeline { stages: vec![] }));
        }

        // Find where the stream selector ends. It is the content up to the
        // closing `}` of the outermost brace pair, or the whole string if
        // there are no braces.
        let (selector, rest) = split_selector(input);

        let stages = parse_stages(rest)?;
        Ok((selector.trim().to_string(), Pipeline { stages }))
    }

    /// Apply the pipeline to a slice of log entries, returning the
    /// processed result.
    pub fn apply(&self, entries: &[LogEntry]) -> Vec<LogEntry> {
        let mut result: Vec<LogEntry> = entries.to_vec();
        for stage in &self.stages {
            result = apply_stage(stage, &result);
        }
        result
    }

    /// Returns `true` if the pipeline contains no stages.
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Returns the stages in this pipeline.
    pub fn stages(&self) -> &[PipelineStage] {
        &self.stages
    }
}

// ---------------------------------------------------------------------------
// Selector splitting
// ---------------------------------------------------------------------------

/// Split `input` into `(selector, rest)` where the selector is the `{…}`
/// prefix and rest is everything after it (stripped of a leading `|`).
fn split_selector(input: &str) -> (String, &str) {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();

    if chars.first() != Some(&'{') {
        // No braces — the entire input could be pipeline stages only, or
        // a plain selector without braces.  We treat it as: everything up
        // to the first unquoted `|` is the selector.
        return split_at_first_pipe(input);
    }

    let mut depth = 0i32;
    let mut in_quote = false;
    let mut quote_char = '"';
    let mut i = 0usize;

    while i < len {
        let c = chars[i];
        if in_quote {
            if c == '\\' {
                i += 1; // skip escaped char
            } else if c == quote_char {
                in_quote = false;
            }
        } else {
            if c == '"' || c == '\'' {
                in_quote = true;
                quote_char = c;
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    // Selector ends right after this '}'.
                    let byte_end = chars[..=i].iter().collect::<String>().len();
                    let selector = &input[..byte_end];
                    let rest = input[byte_end..].trim_start();
                    // Strip a leading '|' from the rest.
                    let rest = rest.strip_prefix('|').unwrap_or(rest).trim_start();
                    return (selector.to_string(), rest);
                }
            }
        }
        i += 1;
    }

    // Unmatched brace — just treat the whole thing as a selector.
    (input.to_string(), "")
}

/// When there are no braces, split at the first unquoted `|`.
fn split_at_first_pipe(input: &str) -> (String, &str) {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut in_quote = false;
    let mut quote_char = '"';
    let mut byte_offset = 0usize;
    let mut i = 0;

    while i < len {
        let c = chars[i];
        if in_quote {
            if c == '\\' && i + 1 < len {
                // Skip both the backslash and the escaped character.
                byte_offset += c.len_utf8() + chars[i + 1].len_utf8();
                i += 2;
                continue;
            }
            if c == quote_char {
                in_quote = false;
            }
        } else if c == '"' || c == '\'' {
            in_quote = true;
            quote_char = c;
        } else if c == '|' {
            let selector = input[..byte_offset].trim();
            let rest = input[byte_offset + 1..].trim_start();
            return (selector.to_string(), rest);
        }
        byte_offset += c.len_utf8();
        i += 1;
    }

    (input.to_string(), "")
}

// ---------------------------------------------------------------------------
// Stage parsing
// ---------------------------------------------------------------------------

/// Parse the `rest` portion (everything after the stream selector and the
/// first `|`) into a vec of stages. Stages are separated by `|`.
///
/// Additionally, `!=` and `!~` at the start of a sub-token (after a closing
/// quote) are treated as implicit stage separators, matching LogQL syntax
/// where `|= "a" != "b"` is two line filter stages.
fn parse_stages(input: &str) -> Result<Vec<PipelineStage>, String> {
    if input.trim().is_empty() {
        return Ok(vec![]);
    }

    let tokens = split_pipeline_tokens(input);
    let mut stages = Vec::new();

    for token in &tokens {
        // A single pipe-separated token might contain multiple line filters
        // chained without `|`, e.g. `= "error" != "timeout"`.
        let sub_tokens = split_implicit_line_filters(token.trim());
        for sub in &sub_tokens {
            let sub = sub.trim();
            if sub.is_empty() {
                continue;
            }
            stages.push(parse_single_stage(sub)?);
        }
    }

    Ok(stages)
}

/// Split a token on implicit line filter boundaries (`!=` or `!~` that
/// appear after a closing quote, outside any quoted string).
fn split_implicit_line_filters(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = '"';
    let mut i = 0;

    while i < len {
        let c = chars[i];
        if in_quote {
            if c == '\\' && i + 1 < len {
                current.push(c);
                current.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == quote_char {
                in_quote = false;
            }
            current.push(c);
            i += 1;
            continue;
        }

        if c == '"' || c == '\'' || c == '`' {
            in_quote = true;
            quote_char = c;
            current.push(c);
            i += 1;
            continue;
        }

        // Check for `!=` or `!~` that look like a standalone line filter
        // (preceded by whitespace), not part of a label filter like
        // `status!="200"`. Only split if `current` already has content.
        if c == '!'
            && i + 1 < len
            && (chars[i + 1] == '=' || chars[i + 1] == '~')
            && (i == 0 || chars[i - 1].is_whitespace())
            && !current.trim().is_empty()
        {
            tokens.push(current.clone());
            current.clear();
        }

        current.push(c);
        i += 1;
    }

    if !current.trim().is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Split `input` on `|` that are not inside quotes.
fn split_pipeline_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut in_quote = false;
    let mut quote_char = '"';
    let mut i = 0;

    while i < len {
        let c = chars[i];
        if in_quote {
            if c == '\\' && i + 1 < len {
                current.push(c);
                current.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == quote_char {
                in_quote = false;
            }
            current.push(c);
        } else if c == '"' || c == '\'' || c == '`' {
            in_quote = true;
            quote_char = c;
            current.push(c);
        } else if c == '|' {
            tokens.push(current.clone());
            current.clear();
        } else {
            current.push(c);
        }
        i += 1;
    }

    if !current.trim().is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Parse a single stage token (already trimmed, no surrounding `|`).
fn parse_single_stage(token: &str) -> Result<PipelineStage, String> {
    let token = token.trim();

    // --- Line filter operators: |= != |~ !~ ---
    // These appear as `= "text"`, `!= "text"`, `~ "regex"`, `!~ "regex"`
    // because the leading `|` was consumed as a separator.
    // |= "text" — literal substring match
    if let Some(rest) = token.strip_prefix("= ") {
        let pattern = extract_quoted_string(rest.trim())?;
        return Ok(PipelineStage::LineFilter {
            pattern,
            negate: false,
            regex: false,
        });
    }
    // |~ "regex" or =~ "regex" — regex match
    if token.starts_with("=~") || token.starts_with("= ~") {
        let rest = token
            .strip_prefix("=~")
            .or_else(|| token.strip_prefix("= ~"))
            .unwrap_or("");
        let pattern = extract_quoted_string(rest.trim())?;
        return Ok(PipelineStage::LineFilter {
            pattern,
            negate: false,
            regex: true,
        });
    }
    if let Some(rest) = token.strip_prefix("~ ") {
        let pattern = extract_quoted_string(rest.trim())?;
        return Ok(PipelineStage::LineFilter {
            pattern,
            negate: false,
            regex: true,
        });
    }
    if token == "~" {
        return Err("line filter `|~` requires a pattern".to_string());
    }
    // != "text" — negated literal substring match
    if let Some(rest) = token.strip_prefix("!= ") {
        let pattern = extract_quoted_string(rest.trim())?;
        return Ok(PipelineStage::LineFilter {
            pattern,
            negate: true,
            regex: false,
        });
    }
    // !~ "regex" — negated regex match
    if let Some(rest) = token.strip_prefix("!~") {
        let rest = rest.trim();
        let pattern = extract_quoted_string(rest)?;
        return Ok(PipelineStage::LineFilter {
            pattern,
            negate: true,
            regex: true,
        });
    }

    // --- Keyword stages ---
    if token.eq_ignore_ascii_case("json") {
        return Ok(PipelineStage::Json);
    }
    if token.eq_ignore_ascii_case("logfmt") {
        return Ok(PipelineStage::Logfmt);
    }
    if token.eq_ignore_ascii_case("dedup") {
        return Ok(PipelineStage::Dedup);
    }
    if token.eq_ignore_ascii_case("unpack") {
        return Ok(PipelineStage::Unpack);
    }
    if token.eq_ignore_ascii_case("rate") {
        return Ok(PipelineStage::Rate);
    }

    // --- sample N ---
    if let Some(rest) = strip_keyword(token, "sample") {
        let n: usize = rest
            .trim()
            .parse()
            .map_err(|_| "sample requires a positive integer argument".to_string())?;
        if n == 0 {
            return Err("sample requires a positive integer (>= 1)".to_string());
        }
        return Ok(PipelineStage::Sample { n });
    }

    // --- limit N ---
    if let Some(rest) = strip_keyword(token, "limit") {
        let n: usize = rest
            .trim()
            .parse()
            .map_err(|_| "limit requires a positive integer argument".to_string())?;
        return Ok(PipelineStage::Limit { n });
    }

    // --- regex "pattern" ---
    if let Some(rest) = strip_keyword(token, "regex") {
        let pattern = extract_quoted_string(rest.trim())?;
        return Ok(PipelineStage::Regex { pattern });
    }

    // --- line_format "template" ---
    if let Some(rest) = strip_keyword(token, "line_format") {
        let template = extract_quoted_string(rest.trim())?;
        return Ok(PipelineStage::LineFormat { template });
    }

    // --- label_drop a, b, c ---
    if let Some(rest) = strip_keyword(token, "label_drop") {
        let labels = parse_label_list(rest);
        if labels.is_empty() {
            return Err("label_drop requires at least one label".to_string());
        }
        return Ok(PipelineStage::LabelDrop { labels });
    }

    // --- label_keep a, b, c ---
    if let Some(rest) = strip_keyword(token, "label_keep") {
        let labels = parse_label_list(rest);
        if labels.is_empty() {
            return Err("label_keep requires at least one label".to_string());
        }
        return Ok(PipelineStage::LabelKeep { labels });
    }

    // --- label filter: label<op>"value" ---
    if let Some(stage) = try_parse_label_filter(token)? {
        return Ok(stage);
    }

    Err(format!("unrecognized pipeline stage: `{token}`"))
}

/// Try to parse a label filter like `level="error"`, `status!=200`,
/// `method=~"GET|POST"`, `path!~"health"`.
fn try_parse_label_filter(token: &str) -> Result<Option<PipelineStage>, String> {
    // We try the two-char operators first to avoid ambiguity.
    let ops: &[(&str, FilterOp)] = &[
        ("!~", FilterOp::Nre),
        ("=~", FilterOp::Re),
        ("!=", FilterOp::Neq),
        ("=", FilterOp::Eq),
    ];

    for (op_str, op) in ops {
        if let Some(pos) = token.find(op_str) {
            let label = token[..pos].trim();
            if label.is_empty() || !is_valid_label(label) {
                continue;
            }
            let value_part = token[pos + op_str.len()..].trim();
            let value = if value_part.starts_with('"') || value_part.starts_with('\'') {
                extract_quoted_string(value_part)?
            } else {
                value_part.to_string()
            };
            return Ok(Some(PipelineStage::LabelFilter {
                label: label.to_string(),
                op: op.clone(),
                value,
            }));
        }
    }

    Ok(None)
}

/// Check if `s` looks like a valid label name (alphanumeric + underscore).
fn is_valid_label(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// Strip a leading keyword (case-insensitive) and return the remainder.
fn strip_keyword<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    let lower = input.trim().to_lowercase();
    if lower.starts_with(keyword) {
        let rest = &input.trim()[keyword.len()..];
        if rest.is_empty() || rest.starts_with(char::is_whitespace) || rest.starts_with('"') {
            Some(rest)
        } else {
            None
        }
    } else {
        None
    }
}

/// Extract a quoted string value. Accepts `"…"` or `'…'` with backslash
/// escaping. If the input has no quotes, returns it as-is.
fn extract_quoted_string(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("expected a quoted string but found nothing".to_string());
    }

    let first = input.chars().next().unwrap();
    if first != '"' && first != '\'' && first != '`' {
        // Unquoted — return as-is (allow bare words).
        return Ok(input.to_string());
    }

    let quote = first;
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut result = String::new();
    let mut i = 1; // skip opening quote

    while i < len {
        let c = chars[i];
        if c == '\\' && i + 1 < len {
            result.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == quote {
            return Ok(result);
        }
        result.push(c);
        i += 1;
    }

    // No closing quote — be lenient and return what we have.
    Ok(result)
}

/// Parse a comma-separated label list.
fn parse_label_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Stage application
// ---------------------------------------------------------------------------

fn apply_stage(stage: &PipelineStage, entries: &[LogEntry]) -> Vec<LogEntry> {
    match stage {
        PipelineStage::Json => apply_json(entries),
        PipelineStage::Logfmt => apply_logfmt(entries),
        PipelineStage::Regex { pattern } => apply_regex(entries, pattern),
        PipelineStage::LabelFilter { label, op, value } => {
            apply_label_filter(entries, label, op, value)
        }
        PipelineStage::LineFilter { pattern, negate, regex } => {
            apply_line_filter(entries, pattern, *negate, *regex)
        }
        PipelineStage::LineFormat { template } => apply_line_format(entries, template),
        PipelineStage::LabelDrop { labels } => apply_label_drop(entries, labels),
        PipelineStage::LabelKeep { labels } => apply_label_keep(entries, labels),
        PipelineStage::Dedup => apply_dedup(entries),
        PipelineStage::Unpack => apply_unpack(entries),
        PipelineStage::Rate => entries.to_vec(), // no-op marker for metrics system
        PipelineStage::Sample { n } => apply_sample(entries, *n),
        PipelineStage::Limit { n } => entries.iter().take(*n).cloned().collect(),
    }
}

fn apply_json(entries: &[LogEntry]) -> Vec<LogEntry> {
    entries
        .iter()
        .map(|entry| {
            let mut e = entry.clone();
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&entry.line) {
                for (k, v) in &map {
                    let string_val = match v {
                        Value::String(s) => s.clone(),
                        Value::Null => continue,
                        other => other.to_string(),
                    };
                    e.labels.insert(k.clone(), string_val);
                }
            }
            e
        })
        .collect()
}

fn apply_logfmt(entries: &[LogEntry]) -> Vec<LogEntry> {
    entries
        .iter()
        .map(|entry| {
            let mut e = entry.clone();
            for pair in parse_logfmt_pairs(&entry.line) {
                e.labels.insert(pair.0, pair.1);
            }
            e
        })
        .collect()
}

/// Minimal logfmt parser: `key=value` or `key="quoted value"`.
fn parse_logfmt_pairs(line: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace.
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Read key.
        let key_start = i;
        while i < len && chars[i] != '=' && !chars[i].is_whitespace() {
            i += 1;
        }
        let key: String = chars[key_start..i].iter().collect();
        if key.is_empty() || i >= len || chars[i] != '=' {
            // Skip non key=value token.
            while i < len && !chars[i].is_whitespace() {
                i += 1;
            }
            continue;
        }
        i += 1; // skip '='

        if i >= len {
            pairs.push((key, String::new()));
            break;
        }

        let value = if chars[i] == '"' {
            i += 1;
            let mut v = String::new();
            while i < len && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                    v.push(chars[i]);
                } else {
                    v.push(chars[i]);
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            v
        } else {
            let start = i;
            while i < len && !chars[i].is_whitespace() {
                i += 1;
            }
            chars[start..i].iter().collect()
        };

        pairs.push((key, value));
    }

    pairs
}

fn apply_regex(entries: &[LogEntry], pattern: &str) -> Vec<LogEntry> {
    let re = match Regex::new(pattern) {
        Ok(r) => r,
        Err(_) => return entries.to_vec(), // invalid regex — pass through
    };

    entries
        .iter()
        .map(|entry| {
            let mut e = entry.clone();
            if let Some(caps) = re.captures(&entry.line) {
                for name in re.capture_names().flatten() {
                    if let Some(m) = caps.name(name) {
                        e.labels.insert(name.to_string(), m.as_str().to_string());
                    }
                }
            }
            e
        })
        .collect()
}

fn apply_label_filter(
    entries: &[LogEntry],
    label: &str,
    op: &FilterOp,
    value: &str,
) -> Vec<LogEntry> {
    let compiled_re = if matches!(op, FilterOp::Re | FilterOp::Nre) {
        Regex::new(value).ok()
    } else {
        None
    };

    entries
        .iter()
        .filter(|entry| {
            let actual = entry.labels.get(label).map(|s| s.as_str()).unwrap_or("");
            match op {
                FilterOp::Eq => actual == value,
                FilterOp::Neq => actual != value,
                FilterOp::Re => compiled_re
                    .as_ref()
                    .map_or(false, |re| re.is_match(actual)),
                FilterOp::Nre => compiled_re
                    .as_ref()
                    .map_or(true, |re| !re.is_match(actual)),
            }
        })
        .cloned()
        .collect()
}

fn apply_line_filter(entries: &[LogEntry], pattern: &str, negate: bool, is_regex: bool) -> Vec<LogEntry> {
    let compiled_re = if is_regex {
        Regex::new(pattern).ok()
    } else {
        None
    };

    entries
        .iter()
        .filter(|entry| {
            let matches = if let Some(ref re) = compiled_re {
                re.is_match(&entry.line)
            } else {
                // Literal substring match for |= / !=.
                entry.line.contains(pattern)
            };
            if negate { !matches } else { matches }
        })
        .cloned()
        .collect()
}

fn apply_line_format(entries: &[LogEntry], template: &str) -> Vec<LogEntry> {
    entries
        .iter()
        .map(|entry| {
            let mut e = entry.clone();
            let mut line = template.to_string();

            // Replace {{.label}} placeholders.
            for (k, v) in &entry.labels {
                let placeholder = format!("{{{{.{k}}}}}");
                line = line.replace(&placeholder, v);
            }

            // Also support {{ .label }} with spaces.
            for (k, v) in &entry.labels {
                let placeholder = format!("{{{{ .{k} }}}}");
                line = line.replace(&placeholder, v);
            }

            e.line = line;
            e
        })
        .collect()
}

fn apply_label_drop(entries: &[LogEntry], labels: &[String]) -> Vec<LogEntry> {
    entries
        .iter()
        .map(|entry| {
            let mut e = entry.clone();
            for label in labels {
                e.labels.remove(label);
            }
            e
        })
        .collect()
}

fn apply_label_keep(entries: &[LogEntry], labels: &[String]) -> Vec<LogEntry> {
    entries
        .iter()
        .map(|entry| {
            let mut e = entry.clone();
            let keep: std::collections::HashSet<&str> =
                labels.iter().map(|s| s.as_str()).collect();
            e.labels = e
                .labels
                .into_iter()
                .filter(|(k, _)| keep.contains(k.as_str()))
                .collect::<BTreeMap<_, _>>();
            e
        })
        .collect()
}

fn apply_dedup(entries: &[LogEntry]) -> Vec<LogEntry> {
    let mut result = Vec::with_capacity(entries.len());
    let mut last_line: Option<&str> = None;

    for entry in entries {
        if last_line == Some(&entry.line) {
            continue;
        }
        last_line = Some(&entry.line);
        result.push(entry.clone());
    }

    result
}

fn apply_unpack(entries: &[LogEntry]) -> Vec<LogEntry> {
    entries
        .iter()
        .map(|entry| {
            let mut e = entry.clone();
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&entry.line) {
                flatten_json(&map, "", &mut e.labels);
            }
            e
        })
        .collect()
}

/// Recursively flatten a JSON object into dot-notation label keys.
fn flatten_json(
    map: &serde_json::Map<String, Value>,
    prefix: &str,
    labels: &mut BTreeMap<String, String>,
) {
    for (k, v) in map {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            Value::Object(nested) => flatten_json(nested, &key, labels),
            Value::Null => {}
            Value::String(s) => {
                labels.insert(key, s.clone());
            }
            other => {
                labels.insert(key, other.to_string());
            }
        }
    }
}

fn apply_sample(entries: &[LogEntry], n: usize) -> Vec<LogEntry> {
    entries
        .iter()
        .enumerate()
        .filter(|(i, _)| i % n == 0)
        .map(|(_, e)| e.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_entry(line: &str) -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            labels: BTreeMap::new(),
            line: line.to_string(),
            raw: None,
        }
    }

    fn make_entry_with_labels(line: &str, labels: &[(&str, &str)]) -> LogEntry {
        let mut map = BTreeMap::new();
        for (k, v) in labels {
            map.insert(k.to_string(), v.to_string());
        }
        LogEntry {
            timestamp: Utc::now(),
            labels: map,
            line: line.to_string(),
            raw: None,
        }
    }

    #[test]
    fn test_parse_simple_json_stage() {
        let input = r#"{job="api"} | json"#;
        let (selector, pipeline) = Pipeline::parse(input).unwrap();
        assert_eq!(selector, r#"{job="api"}"#);
        assert_eq!(pipeline.stages.len(), 1);
        assert!(matches!(pipeline.stages[0], PipelineStage::Json));
    }

    #[test]
    fn test_parse_multiple_stages() {
        let input = r#"{app="web"} | json | level="error" | line_format "{{.msg}}""#;
        let (selector, pipeline) = Pipeline::parse(input).unwrap();
        assert_eq!(selector, r#"{app="web"}"#);
        assert_eq!(pipeline.stages.len(), 3);
        assert!(matches!(pipeline.stages[0], PipelineStage::Json));
        assert!(matches!(
            pipeline.stages[1],
            PipelineStage::LabelFilter {
                ref label,
                op: FilterOp::Eq,
                ref value,
            } if label == "level" && value == "error"
        ));
        assert!(matches!(
            pipeline.stages[2],
            PipelineStage::LineFormat { ref template } if template == "{{.msg}}"
        ));
    }

    #[test]
    fn test_parse_line_filters() {
        let input = r#"{} |= "error" != "timeout""#;
        let (_, pipeline) = Pipeline::parse(input).unwrap();
        assert_eq!(pipeline.stages.len(), 2);
        assert!(matches!(
            pipeline.stages[0],
            PipelineStage::LineFilter {
                ref pattern,
                negate: false,
                regex: false,
            } if pattern == "error"
        ));
        assert!(matches!(
            pipeline.stages[1],
            PipelineStage::LineFilter {
                ref pattern,
                negate: true,
                regex: false,
            } if pattern == "timeout"
        ));
    }

    #[test]
    fn test_parse_label_operators() {
        let input = r#"{} | json | status!="200" | method=~"GET|POST""#;
        let (_, pipeline) = Pipeline::parse(input).unwrap();
        assert_eq!(pipeline.stages.len(), 3);
        assert!(matches!(
            pipeline.stages[1],
            PipelineStage::LabelFilter {
                ref label,
                op: FilterOp::Neq,
                ref value,
            } if label == "status" && value == "200"
        ));
        assert!(matches!(
            pipeline.stages[2],
            PipelineStage::LabelFilter {
                ref label,
                op: FilterOp::Re,
                ref value,
            } if label == "method" && value == "GET|POST"
        ));
    }

    #[test]
    fn test_parse_label_drop_keep_dedup() {
        let input = r#"{} | label_drop host, region | label_keep level | dedup"#;
        let (_, pipeline) = Pipeline::parse(input).unwrap();
        assert_eq!(pipeline.stages.len(), 3);
        assert!(matches!(
            pipeline.stages[0],
            PipelineStage::LabelDrop { ref labels } if labels == &["host", "region"]
        ));
        assert!(matches!(
            pipeline.stages[1],
            PipelineStage::LabelKeep { ref labels } if labels == &["level"]
        ));
        assert!(matches!(pipeline.stages[2], PipelineStage::Dedup));
    }

    #[test]
    fn test_apply_json_stage() {
        let entries = vec![make_entry(r#"{"msg":"hello","level":"info"}"#)];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::Json],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].labels.get("msg").map(|s| s.as_str()), Some("hello"));
        assert_eq!(result[0].labels.get("level").map(|s| s.as_str()), Some("info"));
    }

    #[test]
    fn test_apply_logfmt_stage() {
        let entries = vec![make_entry("level=error msg=\"connection failed\" duration=42ms")];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::Logfmt],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result[0].labels.get("level").map(|s| s.as_str()), Some("error"));
        assert_eq!(
            result[0].labels.get("msg").map(|s| s.as_str()),
            Some("connection failed")
        );
    }

    #[test]
    fn test_apply_label_filter() {
        let entries = vec![
            make_entry_with_labels("ok", &[("level", "info")]),
            make_entry_with_labels("bad", &[("level", "error")]),
            make_entry_with_labels("warn", &[("level", "warn")]),
        ];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::LabelFilter {
                label: "level".to_string(),
                op: FilterOp::Eq,
                value: "error".to_string(),
            }],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, "bad");
    }

    #[test]
    fn test_apply_line_filter_negate() {
        let entries = vec![
            make_entry("connection timeout"),
            make_entry("request completed"),
            make_entry("timeout on read"),
        ];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::LineFilter {
                regex: false,
                pattern: "timeout".to_string(),
                negate: true,
            }],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, "request completed");
    }

    #[test]
    fn test_apply_dedup() {
        let entries = vec![
            make_entry("hello"),
            make_entry("hello"),
            make_entry("world"),
            make_entry("world"),
            make_entry("hello"),
        ];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::Dedup],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].line, "hello");
        assert_eq!(result[1].line, "world");
        assert_eq!(result[2].line, "hello");
    }

    #[test]
    fn test_apply_line_format() {
        let entries = vec![make_entry_with_labels(
            "original",
            &[("level", "info"), ("msg", "hello world")],
        )];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::LineFormat {
                template: "{{.level}}: {{.msg}}".to_string(),
            }],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result[0].line, "info: hello world");
    }

    #[test]
    fn test_apply_regex_named_captures() {
        let entries = vec![make_entry("192.168.1.1 GET /api/health 200 12ms")];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::Regex {
                pattern: r"(?P<ip>\S+) (?P<method>\S+) (?P<path>\S+) (?P<status>\d+)".to_string(),
            }],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result[0].labels.get("ip").map(|s| s.as_str()), Some("192.168.1.1"));
        assert_eq!(result[0].labels.get("method").map(|s| s.as_str()), Some("GET"));
        assert_eq!(result[0].labels.get("status").map(|s| s.as_str()), Some("200"));
    }

    #[test]
    fn test_apply_label_drop_and_keep() {
        let entries = vec![make_entry_with_labels(
            "test",
            &[("level", "info"), ("host", "web-01"), ("region", "us-east")],
        )];

        // Drop host and region.
        let p1 = Pipeline {
            stages: vec![PipelineStage::LabelDrop {
                labels: vec!["host".to_string(), "region".to_string()],
            }],
        };
        let r1 = p1.apply(&entries);
        assert!(r1[0].labels.contains_key("level"));
        assert!(!r1[0].labels.contains_key("host"));
        assert!(!r1[0].labels.contains_key("region"));

        // Keep only level.
        let p2 = Pipeline {
            stages: vec![PipelineStage::LabelKeep {
                labels: vec!["level".to_string()],
            }],
        };
        let r2 = p2.apply(&entries);
        assert!(r2[0].labels.contains_key("level"));
        assert!(!r2[0].labels.contains_key("host"));
    }

    #[test]
    fn test_empty_input() {
        let (selector, pipeline) = Pipeline::parse("").unwrap();
        assert_eq!(selector, "");
        assert!(pipeline.is_empty());
    }

    #[test]
    fn test_selector_only() {
        let (selector, pipeline) = Pipeline::parse(r#"{job="api"}"#).unwrap();
        assert_eq!(selector, r#"{job="api"}"#);
        assert!(pipeline.is_empty());
    }

    #[test]
    fn test_full_pipeline_integration() {
        let entries = vec![
            make_entry(r#"{"level":"error","msg":"disk full","host":"web-01"}"#),
            make_entry(r#"{"level":"info","msg":"request ok","host":"web-02"}"#),
            make_entry(r#"{"level":"error","msg":"disk full","host":"web-01"}"#),
        ];

        let input = r#"{} | json | level="error" | label_drop host | dedup"#;
        let (_, pipeline) = Pipeline::parse(input).unwrap();
        let result = pipeline.apply(&entries);

        // After json: labels promoted. After level=error: only 2 entries.
        // After label_drop host: host removed. After dedup: 1 entry.
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].labels.get("msg").map(|s| s.as_str()),
            Some("disk full")
        );
        assert!(!result[0].labels.contains_key("host"));
    }

    // --- Tests for new pipeline stages ---

    #[test]
    fn test_parse_unpack() {
        let input = r#"{} | unpack"#;
        let (_, pipeline) = Pipeline::parse(input).unwrap();
        assert_eq!(pipeline.stages.len(), 1);
        assert!(matches!(pipeline.stages[0], PipelineStage::Unpack));
    }

    #[test]
    fn test_parse_rate() {
        let input = r#"{} | rate"#;
        let (_, pipeline) = Pipeline::parse(input).unwrap();
        assert_eq!(pipeline.stages.len(), 1);
        assert!(matches!(pipeline.stages[0], PipelineStage::Rate));
    }

    #[test]
    fn test_parse_sample() {
        let input = r#"{} | sample 10"#;
        let (_, pipeline) = Pipeline::parse(input).unwrap();
        assert_eq!(pipeline.stages.len(), 1);
        assert!(matches!(pipeline.stages[0], PipelineStage::Sample { n: 10 }));
    }

    #[test]
    fn test_parse_limit() {
        let input = r#"{} | limit 100"#;
        let (_, pipeline) = Pipeline::parse(input).unwrap();
        assert_eq!(pipeline.stages.len(), 1);
        assert!(matches!(pipeline.stages[0], PipelineStage::Limit { n: 100 }));
    }

    #[test]
    fn test_apply_unpack_nested_json() {
        let entries = vec![make_entry(
            r#"{"msg":"hello","meta":{"region":"us","dc":{"id":1}}}"#,
        )];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::Unpack],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].labels.get("msg").map(|s| s.as_str()),
            Some("hello")
        );
        assert_eq!(
            result[0].labels.get("meta.region").map(|s| s.as_str()),
            Some("us")
        );
        assert_eq!(
            result[0].labels.get("meta.dc.id").map(|s| s.as_str()),
            Some("1")
        );
    }

    #[test]
    fn test_apply_unpack_non_json() {
        let entries = vec![make_entry("plain text line")];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::Unpack],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result.len(), 1);
        assert!(result[0].labels.is_empty());
    }

    #[test]
    fn test_apply_rate_passthrough() {
        let entries = vec![make_entry("a"), make_entry("b"), make_entry("c")];
        let pipeline = Pipeline {
            stages: vec![PipelineStage::Rate],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_apply_sample() {
        let entries: Vec<LogEntry> = (0..10).map(|i| make_entry(&format!("line{i}"))).collect();
        let pipeline = Pipeline {
            stages: vec![PipelineStage::Sample { n: 3 }],
        };
        let result = pipeline.apply(&entries);
        // Keeps indices 0, 3, 6, 9
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].line, "line0");
        assert_eq!(result[1].line, "line3");
        assert_eq!(result[2].line, "line6");
        assert_eq!(result[3].line, "line9");
    }

    #[test]
    fn test_apply_limit() {
        let entries: Vec<LogEntry> = (0..10).map(|i| make_entry(&format!("line{i}"))).collect();
        let pipeline = Pipeline {
            stages: vec![PipelineStage::Limit { n: 3 }],
        };
        let result = pipeline.apply(&entries);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].line, "line0");
        assert_eq!(result[1].line, "line1");
        assert_eq!(result[2].line, "line2");
    }

    #[test]
    fn test_combined_new_stages() {
        let entries: Vec<LogEntry> = (0..20).map(|i| make_entry(&format!("line{i}"))).collect();
        let input = r#"{} | sample 5 | limit 2"#;
        let (_, pipeline) = Pipeline::parse(input).unwrap();
        let result = pipeline.apply(&entries);
        // sample 5 keeps: 0, 5, 10, 15 -> limit 2 keeps: 0, 5
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, "line0");
        assert_eq!(result[1].line, "line5");
    }
}
