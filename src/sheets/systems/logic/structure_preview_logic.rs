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
                            .filter(|(k, v)| {
                                !v.trim().is_empty()
                                    && !k.eq_ignore_ascii_case("__parentdescriptor")
                            })
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
                    .filter(|(k, v)| {
                        !v.trim().is_empty() && !k.eq_ignore_ascii_case("__parentdescriptor")
                    })
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

/// Generate a preview string from structure rows (Vec<Vec<String>>)
/// Similar to generate_structure_preview but takes rows directly instead of JSON
/// Skips the first 2 columns (row_index at 0, parent_key at 1) to show only data columns
pub fn generate_structure_preview_from_rows(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let first_row = &rows[0];
    // Skip first 2 columns (row_index and parent_key) - start from index 2
    let values: Vec<String> = first_row
        .iter()
        .skip(2)
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
