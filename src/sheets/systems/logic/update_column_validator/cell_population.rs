// src/sheets/systems/logic/update_column_validator/cell_population.rs
// Grid cell population and manipulation for structure columns

use bevy::prelude::*;
use serde_json::Value;

use crate::sheets::events::SheetOperationFeedback;

/// Populate rows when converting TO Structure
pub fn populate_structure_rows(
    grid: &mut [Vec<String>],
    col_index: usize,
    value_sources: &[(usize, bool)],
    old_self_cells: &[String],
) {
    for (row_idx, row) in grid.iter_mut().enumerate() {
        if row.len() <= col_index {
            row.resize(col_index + 1, String::new());
        }
        if value_sources.is_empty() {
            row[col_index] = "[]".to_string();
        } else {
            let mut vec_vals: Vec<Value> = Vec::with_capacity(value_sources.len());
            for (src_idx, is_self) in value_sources.iter() {
                if *is_self {
                    // Use pre-conversion cell value
                    let val = old_self_cells.get(row_idx).cloned().unwrap_or_default();
                    vec_vals.push(Value::String(val));
                } else {
                    let val = row.get(*src_idx).cloned().unwrap_or_default();
                    vec_vals.push(Value::String(val));
                }
            }
            row[col_index] = Value::Array(vec_vals).to_string();
        }
    }
}

/// Ensure existing Structure cells are not empty
pub fn ensure_structure_cells_not_empty(grid: &mut [Vec<String>], col_index: usize) {
    for row in grid.iter_mut() {
        if row.len() <= col_index {
            row.resize(col_index + 1, String::new());
        }
        if let Some(cell) = row.get_mut(col_index) {
            if cell.trim().is_empty() {
                *cell = "[]".to_string();
            }
        }
    }
}

/// Handle conversion FROM Structure validator
/// Flattens JSON objects into readable strings
pub fn handle_structure_conversion_from(
    grid: &mut [Vec<String>],
    col_index: usize,
    column_header: &str,
    feedback_writer: &mut bevy::prelude::EventWriter<SheetOperationFeedback>,
) {
    // Conversion AWAY from Structure: flatten existing JSON object content into semi-readable single-line string.
    // We keep data but warn about potential loss of structured editing.
    let warn_msg = format!(
        "Warning: Converted Structure column '{}' to new validator. JSON objects flattened into 'key=value; key2=value2' strings (data may no longer be editable as structured).",
        column_header
    );
    warn!("{}", warn_msg);
    feedback_writer.write(SheetOperationFeedback {
        message: warn_msg,
        is_error: false,
    });

    for row in grid.iter_mut() {
        if row.len() <= col_index {
            continue;
        }
        if let Some(cell) = row.get_mut(col_index) {
            let trimmed = cell.trim();
            if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                match val {
                    Value::Object(map) => {
                        let mut parts: Vec<String> = map
                            .iter()
                            .map(|(k, v)| {
                                format!("{}={}", k, v.as_str().unwrap_or(&v.to_string()))
                            })
                            .collect();
                        parts.sort();
                        *cell = parts.join("; ");
                    }
                    Value::Array(arr) => {
                        // Array of strings => join; Array of arrays => join rows with |; Array of objects => legacy -> key=value pairs per obj
                        if arr.iter().all(|v| v.is_string()) {
                            let parts: Vec<String> = arr
                                .iter()
                                .map(|v| v.as_str().unwrap_or("").to_string())
                                .collect();
                            *cell = parts.join("; ");
                        } else if arr.iter().all(|v| v.is_array()) {
                            let row_strings: Vec<String> = arr
                                .iter()
                                .map(|row| {
                                    if let Value::Array(inner) = row {
                                        inner
                                            .iter()
                                            .map(|v| v.as_str().unwrap_or(""))
                                            .collect::<Vec<_>>()
                                            .join(";")
                                    } else {
                                        String::new()
                                    }
                                })
                                .collect();
                            *cell = row_strings.join(" | ");
                        } else if arr.iter().all(|v| v.is_object()) {
                            let mut rows: Vec<String> = Vec::new();
                            for obj in arr {
                                if let Value::Object(m) = obj {
                                    let mut parts: Vec<String> = m
                                        .iter()
                                        .map(|(k, v)| {
                                            format!(
                                                "{}={}",
                                                k,
                                                v.as_str().unwrap_or(&v.to_string())
                                            )
                                        })
                                        .collect();
                                    parts.sort();
                                    rows.push(parts.join(";"));
                                }
                            }
                            *cell = rows.join(" | ");
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
