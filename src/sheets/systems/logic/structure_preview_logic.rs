// src/sheets/systems/logic/structure_preview_logic.rs
//! JSON parsing and structure preview generation helpers.
//! These functions handle parsing structure cell data and generating
//! concise preview strings for display in the UI.

/// Generate a concise preview string for a structure cell, matching the grid view rendering.
/// Returns a tuple of (preview_text, parse_failed_flag).
pub fn generate_structure_preview(raw: &str) -> (String, bool) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (String::new(), false);
    }

    let mut out = String::new();
    let mut multi_rows = false;
    let mut parse_failed = false;

    fn stringify_json_value(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.to_owned(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        }
    }

    fn is_technical_key(key: &str) -> bool {
        let lower = key.to_ascii_lowercase();
        lower == "row_index"
            || lower == "parent_key"
            || lower == "temp_new_row_index"
            || lower == "_obsolete_temp_new_row_index"
            || lower == "id"
            || lower == "created_at"
            || lower == "updated_at"
            || lower == "__parentdescriptor"
    }

    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(val) => match val {
            serde_json::Value::Array(arr) => {
                if arr.iter().all(|v| v.is_string()) {
                    let vals: Vec<String> = arr
                        .iter()
                        .map(stringify_json_value)
                        .filter(|s| !s.trim().is_empty())
                        .collect();
                    out = vals.join(", ");
                } else if arr.iter().all(|v| v.is_array()) {
                    multi_rows = arr.len() > 1;
                    if let Some(first) = arr.first().and_then(|v| v.as_array()) {
                        let vals: Vec<String> = first
                            .iter()
                            .map(stringify_json_value)
                            .filter(|s| !s.trim().is_empty())
                            .collect();
                        out = vals.join(", ");
                    }
                } else if arr.iter().all(|v| v.is_object()) {
                    multi_rows = arr.len() > 1;
                    if let Some(first) = arr.first().and_then(|v| v.as_object()) {
                        let mut entries: Vec<(String, String)> = first
                            .iter()
                            .map(|(k, v)| (k.clone(), stringify_json_value(v)))
                            .filter(|(k, v)| !v.trim().is_empty() && !is_technical_key(k))
                            .collect();
                        entries.sort_by(|a, b| a.0.cmp(&b.0));
                        out = entries
                            .into_iter()
                            .map(|(_, v)| v)
                            .collect::<Vec<_>>()
                            .join(", ");
                    }
                } else {
                    let vals: Vec<String> = arr
                        .iter()
                        .map(stringify_json_value)
                        .filter(|s| !s.trim().is_empty())
                        .collect();
                    multi_rows = arr.len() > 1;
                    out = vals.join(", ");
                }
            }
            serde_json::Value::Object(map) => {
                let mut entries: Vec<(String, String)> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), stringify_json_value(v)))
                    .filter(|(k, v)| !v.trim().is_empty() && !is_technical_key(k))
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                out = entries
                    .into_iter()
                    .map(|(_, v)| v)
                    .collect::<Vec<_>>()
                    .join(", ");
            }
            other => {
                out = stringify_json_value(&other);
            }
        },
        Err(_) => parse_failed = true,
    }

    if out.chars().count() > 64 {
        let truncated: String = out.chars().take(64).collect();
        out = truncated + "…";
    }
    if multi_rows {
        out.push_str("...");
    }
    (out, parse_failed)
}

/// Generate a preview string from structure rows with optional headers
/// Skips technical columns based on header names or position
pub fn generate_structure_preview_from_rows_with_headers(
    rows: &[Vec<String>],
    headers: Option<&[String]>,
) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let first_row = &rows[0];
    
    // Determine skip count based on headers if available
    let skip_count = if let Some(hdrs) = headers {
        // Count technical columns: row_index, parent_key
        hdrs.iter()
            .take_while(|h| {
                h.eq_ignore_ascii_case("row_index")
                    || h.eq_ignore_ascii_case("parent_key")
            })
            .count()
    } else {
        // Default: skip row_index and parent_key (first 2 columns)
        2
    };

    // Skip technical columns and collect data values
    let values: Vec<String> = first_row
        .iter()
        .skip(skip_count)
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect();

    let mut out = values.join(", ");

    if out.chars().count() > 64 {
        let truncated: String = out.chars().take(64).collect();
        out = truncated + "…";
    }
    if rows.len() > 1 {
        out.push_str("...");
    }
    out
}
