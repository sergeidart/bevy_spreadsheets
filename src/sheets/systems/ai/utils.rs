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
                let mut out = Vec::with_capacity(arr.len());
                for obj in arr {
                    if let serde_json::Value::Object(map) = obj {
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
