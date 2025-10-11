// src/sheets/systems/ai/utils.rs
// Shared helpers for AI systems (non-UI)
use crate::sheets::definitions::StructureFieldDefinition;

/// Parse structure rows from a serialized cell string produced previously by serialize_structure_rows_for_review.
/// Accepts either a JSON array of objects or legacy newline/pipe/tab separated formats.
pub fn parse_structure_rows_from_cell(
    cell: &str,
    schema: &[StructureFieldDefinition],
) -> Vec<Vec<String>> {
    let trimmed = cell.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // Try JSON first
    if trimmed.starts_with('[') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let serde_json::Value::Array(arr) = val {
                // Check if this is a flat array of values (single row) or array of rows
                // Flat array: ["val1", "val2", "val3"] - all elements are primitives
                // Array of rows: [["val1", "val2"], ...] or [{...}, ...] - elements are arrays/objects
                let is_flat_array = arr.iter().all(|item| {
                    matches!(
                        item,
                        serde_json::Value::String(_)
                            | serde_json::Value::Number(_)
                            | serde_json::Value::Bool(_)
                            | serde_json::Value::Null
                    )
                });

                if is_flat_array && !arr.is_empty() {
                    // Handle flat array as a single row: ["val1", "val2", "val3"]
                    let mut row: Vec<String> = Vec::with_capacity(schema.len());
                    for (idx, val) in arr.iter().enumerate() {
                        if idx < schema.len() {
                            let cell_val = match val {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::Bool(b) => b.to_string(),
                                serde_json::Value::Null => String::new(),
                                _ => String::new(),
                            };
                            row.push(cell_val);
                        }
                    }
                    // Pad row if needed
                    while row.len() < schema.len() {
                        row.push(String::new());
                    }
                    return vec![row];
                }

                // Handle array of rows
                let mut out = Vec::with_capacity(arr.len());
                for item in arr {
                    match item {
                        // Handle array-of-objects format: [{"header1": "val1", "header2": "val2"}, ...]
                        serde_json::Value::Object(map) => {
                            let mut row: Vec<String> = Vec::with_capacity(schema.len());
                            for field in schema {
                                let v = map
                                    .get(&field.header)
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                row.push(v.to_string());
                            }
                            out.push(row);
                        }
                        // Handle array-of-arrays format: [["val1", "val2"], ["val3", "val4"], ...]
                        serde_json::Value::Array(inner_arr) => {
                            let mut row: Vec<String> = Vec::with_capacity(schema.len());
                            for (idx, val) in inner_arr.iter().enumerate() {
                                if idx < schema.len() {
                                    let cell_val = match val {
                                        serde_json::Value::String(s) => s.clone(),
                                        serde_json::Value::Number(n) => n.to_string(),
                                        serde_json::Value::Bool(b) => b.to_string(),
                                        serde_json::Value::Null => String::new(),
                                        _ => String::new(),
                                    };
                                    row.push(cell_val);
                                }
                            }
                            // Pad row if needed
                            while row.len() < schema.len() {
                                row.push(String::new());
                            }
                            out.push(row);
                        }
                        _ => {} // Skip other types
                    }
                }
                return out;
            }
        }
    }
    // Fallback: split lines, then split by tab or pipe
    let mut rows = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<String> = if line.contains('\t') {
            line.split('\t').map(|s| s.trim().to_string()).collect()
        } else if line.contains('|') {
            line.split('|').map(|s| s.trim().to_string()).collect()
        } else {
            vec![line.to_string()]
        };
        if !parts.is_empty() {
            let mut row = parts;
            if row.len() < schema.len() {
                row.resize(schema.len(), String::new());
            }
            rows.push(row);
        }
    }
    rows
}
