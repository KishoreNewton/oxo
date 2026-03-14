//! Multi-line log grouping.
//!
//! Groups continuation lines (stack traces, tracebacks, etc.) with their
//! parent log entry. A continuation line is one that:
//! - Starts with whitespace (indented stack frames)
//! - Starts with "at " (Java/JS stack traces)
//! - Starts with "Caused by:" (Java chained exceptions)
//! - Starts with "File " or "  File " (Python tracebacks)
//! - Starts with "goroutine " (Go panics)
//! - Starts with "|" or "+" (formatted/boxed output)

use crate::LogEntry;

/// A log entry that may have continuation lines grouped with it.
#[derive(Debug, Clone)]
pub struct GroupedEntry {
    /// The parent log entry.
    pub entry: LogEntry,
    /// Continuation lines attached to this entry (stack trace frames, etc.).
    pub continuation_lines: Vec<String>,
    /// Whether the continuation lines are collapsed (hidden) in the UI.
    pub collapsed: bool,
}

/// Determine whether a log line looks like a continuation of a previous entry.
///
/// This catches common multi-line patterns from Java, Python, Rust, Go, and
/// generic formatted output.
fn is_continuation(line: &str) -> bool {
    // Empty or whitespace-only lines between stack frames are continuations.
    if line.is_empty() {
        return false;
    }

    // Starts with a tab or 2+ spaces (indented stack frames).
    if line.starts_with('\t') {
        return true;
    }
    if line.len() >= 2 && line.starts_with("  ") {
        return true;
    }

    let trimmed = line.trim_start();

    // Java / JS: "at com.example.Foo.bar(Foo.java:42)"
    if trimmed.len() >= 3 {
        let lower_start: String = trimmed.chars().take(3).collect();
        if lower_start.eq_ignore_ascii_case("at ") {
            return true;
        }
    }

    // Java chained exceptions: "Caused by: ..."
    if trimmed.starts_with("Caused by:") {
        return true;
    }

    // Rust panic chain: "--- ..."
    if trimmed.starts_with("--- ") {
        return true;
    }

    // Python traceback: 'File "...'
    if trimmed.starts_with("File \"") {
        return true;
    }

    // Go panics: "goroutine N [...]:"
    if trimmed.starts_with("goroutine ") {
        return true;
    }

    // Formatted / boxed output: lines starting with | or +
    if trimmed.starts_with('|') || trimmed.starts_with('+') {
        return true;
    }

    // Lines that look like stack frame addresses: "0x..." or "#N 0x..."
    if trimmed.starts_with("0x") {
        return true;
    }
    if let Some(stripped) = trimmed.strip_prefix('#') {
        // C/C++ gdb-style frames: "#0  0x..."
        let rest = stripped.trim_start();
        if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return true;
        }
    }

    // "... N more" (Java truncated stack traces)
    if trimmed.starts_with("... ") && trimmed.ends_with(" more") {
        return true;
    }

    false
}

/// Determine whether a [`LogEntry`] is likely a continuation of the previous
/// entry based on its label metadata.
///
/// If the entry has no "level" label but the previous entry does, it is likely
/// a continuation line produced by a multi-line log message.
fn is_continuation_entry(entry: &LogEntry, prev: Option<&LogEntry>) -> bool {
    // First check the line content itself.
    if is_continuation(&entry.line) {
        return true;
    }

    // If the previous entry had a level label but this one does not,
    // it is likely a continuation (e.g. a stack frame that the log
    // pipeline emitted as a separate entry without level metadata).
    if let Some(prev) = prev {
        let prev_has_level =
            prev.labels.contains_key("level") || prev.labels.contains_key("severity");
        let this_has_level =
            entry.labels.contains_key("level") || entry.labels.contains_key("severity");
        if prev_has_level && !this_has_level {
            return true;
        }
    }

    false
}

/// Group a slice of log entries by attaching continuation lines to their
/// parent entry.
///
/// Walks entries in order. When a line is detected as a continuation (via
/// [`is_continuation`] heuristics or missing level labels), it is appended
/// to the previous [`GroupedEntry::continuation_lines`] instead of becoming
/// a standalone entry.
pub fn group_entries(entries: &[LogEntry]) -> Vec<GroupedEntry> {
    let mut groups: Vec<GroupedEntry> = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        let prev = if i > 0 { Some(&entries[i - 1]) } else { None };

        if !groups.is_empty() && is_continuation_entry(entry, prev) {
            // Attach to the last group as a continuation line.
            let last = groups.last_mut().unwrap();
            last.continuation_lines.push(entry.line.clone());
        } else {
            // Start a new group.
            groups.push(GroupedEntry {
                entry: entry.clone(),
                continuation_lines: Vec::new(),
                collapsed: true,
            });
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::BTreeMap;

    fn make_entry(line: &str, level: Option<&str>) -> LogEntry {
        let mut labels = BTreeMap::new();
        if let Some(l) = level {
            labels.insert("level".to_string(), l.to_string());
        }
        LogEntry {
            timestamp: Utc::now(),
            labels,
            line: line.to_string(),
            raw: None,
        }
    }

    #[test]
    fn test_java_stack_trace() {
        let entries = vec![
            make_entry(
                "java.lang.NullPointerException: something was null",
                Some("error"),
            ),
            make_entry("\tat com.example.Foo.bar(Foo.java:42)", None),
            make_entry("\tat com.example.Main.main(Main.java:10)", None),
            make_entry("Caused by: java.io.IOException: disk full", None),
            make_entry("\tat com.example.IO.write(IO.java:99)", None),
            make_entry("Application started successfully", Some("info")),
        ];

        let groups = group_entries(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].continuation_lines.len(), 4);
        assert!(groups[0].entry.line.contains("NullPointerException"));
        assert_eq!(groups[1].continuation_lines.len(), 0);
        assert!(groups[1].entry.line.contains("Application started"));
    }

    #[test]
    fn test_python_traceback() {
        let entries = vec![
            make_entry("Traceback (most recent call last):", Some("error")),
            make_entry("  File \"app.py\", line 42, in main", None),
            make_entry("    result = do_something()", None),
            make_entry("  File \"lib.py\", line 10, in do_something", None),
            make_entry("    raise ValueError(\"bad value\")", None),
            make_entry("ValueError: bad value", Some("error")),
        ];

        let groups = group_entries(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].continuation_lines.len(), 4);
        assert!(groups[0].entry.line.contains("Traceback"));
    }

    #[test]
    fn test_regular_logs_not_grouped() {
        let entries = vec![
            make_entry("Starting server on port 8080", Some("info")),
            make_entry("Connected to database", Some("info")),
            make_entry("Request received: GET /api/health", Some("debug")),
        ];

        let groups = group_entries(&entries);
        assert_eq!(groups.len(), 3);
        for g in &groups {
            assert!(g.continuation_lines.is_empty());
        }
    }

    #[test]
    fn test_rust_panic_chain() {
        let entries = vec![
            make_entry(
                "thread 'main' panicked at 'index out of bounds'",
                Some("error"),
            ),
            make_entry("--- src/main.rs:42", None),
            make_entry("   0: std::panicking::begin_panic", None),
            make_entry("   1: myapp::process", None),
            make_entry("Server shutting down", Some("info")),
        ];

        let groups = group_entries(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].continuation_lines.len(), 3);
    }

    #[test]
    fn test_is_continuation_basic() {
        assert!(is_continuation("\tat com.Foo.bar(Foo.java:1)"));
        assert!(is_continuation("  at com.Foo.bar(Foo.java:1)"));
        assert!(is_continuation("    raise ValueError()"));
        assert!(is_continuation("Caused by: java.io.IOException"));
        assert!(is_continuation("--- src/main.rs:42"));
        assert!(is_continuation("  File \"app.py\", line 1"));
        assert!(is_continuation("goroutine 1 [running]:"));
        assert!(is_continuation("| some boxed output"));
        assert!(is_continuation("+ another box line"));
        assert!(!is_continuation("Normal log line"));
        assert!(!is_continuation("ERROR something went wrong"));
    }

    #[test]
    fn test_empty_entries() {
        let entries: Vec<LogEntry> = vec![];
        let groups = group_entries(&entries);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_go_panic() {
        let entries = vec![
            make_entry("panic: runtime error: index out of range", Some("error")),
            make_entry("goroutine 1 [running]:", None),
            make_entry("  main.main()", None),
            make_entry("  \t/app/main.go:10 +0x50", None),
            make_entry("exit status 2", Some("error")),
        ];

        let groups = group_entries(&entries);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].continuation_lines.len(), 3);
    }
}
