//! Log export in multiple formats.
//!
//! Supports exporting log entries to JSON, CSV, and NDJSON (newline-delimited
//! JSON) files. Used by the [`App`](crate::app::App) when the user triggers an
//! export action.

use std::io::Write;

use oxo_core::LogEntry;

/// Supported export file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Pretty-printed JSON array.
    Json,
    /// Comma-separated values with header row.
    Csv,
    /// Newline-delimited JSON (one compact object per line).
    Ndjson,
}

impl ExportFormat {
    /// File extension for this format (without leading dot).
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Ndjson => "ndjson",
        }
    }
}

/// Export log entries to a file in the given format.
///
/// Returns the number of entries written on success.
pub fn export_entries(
    entries: &[&LogEntry],
    format: ExportFormat,
    path: &str,
) -> anyhow::Result<usize> {
    let count = entries.len();
    let mut file = std::fs::File::create(path)?;

    match format {
        ExportFormat::Json => {
            let json = serde_json::to_string_pretty(&entries)?;
            file.write_all(json.as_bytes())?;
        }
        ExportFormat::Csv => {
            // Write header.
            file.write_all(b"timestamp,level,labels,line\n")?;
            for entry in entries {
                let timestamp = entry.timestamp.to_rfc3339();
                let level = entry
                    .labels
                    .get("level")
                    .or_else(|| entry.labels.get("severity"))
                    .or_else(|| entry.labels.get("lvl"))
                    .cloned()
                    .unwrap_or_default();
                let labels = entry
                    .labels
                    .iter()
                    .filter(|(k, _)| !matches!(k.as_str(), "level" | "severity" | "lvl"))
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(";");
                writeln!(
                    file,
                    "{},{},{},{}",
                    csv_escape(&timestamp),
                    csv_escape(&level),
                    csv_escape(&labels),
                    csv_escape(&entry.line),
                )?;
            }
        }
        ExportFormat::Ndjson => {
            for entry in entries {
                let json = serde_json::to_string(entry)?;
                file.write_all(json.as_bytes())?;
                file.write_all(b"\n")?;
            }
        }
    }

    file.flush()?;
    Ok(count)
}

/// Escape a field value for CSV output.
///
/// If the value contains a comma, double-quote, or newline the whole field is
/// wrapped in double-quotes with interior quotes doubled per RFC 4180.
fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        let escaped = value.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        value.to_string()
    }
}
