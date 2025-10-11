// src/sheets/systems/logic/update_column_validator/handlers.rs
// Structure-specific conversion handlers

use bevy::prelude::*;
use serde_json::Value;

use crate::sheets::{
    definitions::{ColumnDataType, ColumnDefinition, ColumnValidator, StructureFieldDefinition},
    events::{RequestUpdateColumnValidator, SheetOperationFeedback},
};

/// Handle conversion TO Structure validator
/// Returns (collected_defs, value_sources, structure_columns) for sheet creation
pub fn handle_structure_conversion_to(
    event: &RequestUpdateColumnValidator,
    col_index: usize,
    old_col_def_snapshot: &ColumnDefinition,
    columns_snapshot: &[ColumnDefinition],
    _sheet_name: &str,
    _column_header: &str,
) -> Option<(
    Vec<StructureFieldDefinition>,
    Vec<(usize, bool)>,
    Vec<ColumnDefinition>,
)> {
    let sources = event.structure_source_columns.as_ref()?;

    // Pre-collect source column definitions to avoid borrowing issues
    let mut seen = std::collections::HashSet::new();
    let mut collected_defs: Vec<StructureFieldDefinition> = Vec::new();
    // (src_index, is_self)
    let mut value_sources: Vec<(usize, bool)> = Vec::new();

    for src in sources.iter().copied() {
        if seen.insert(src) {
            if src == col_index {
                // Use old column definition (pre-conversion) so inner field reflects old Basic/Linked type
                let mut def = StructureFieldDefinition::from(old_col_def_snapshot);
                // If UI supplied explicit original validator, prefer it (old_col_def_snapshot might already be Structure if premature mutation elsewhere)
                if let Some(orig) = event.original_self_validator.clone() {
                    def.validator = Some(orig.clone());
                    def.data_type = match orig {
                        ColumnValidator::Basic(t) => t,
                        ColumnValidator::Linked { .. } => ColumnDataType::String,
                        ColumnValidator::Structure => ColumnDataType::String,
                    };
                }
                // Never allow nested structure-of-structure for the self snapshot: if validator still Structure downgrade to String basic
                if matches!(def.validator, Some(ColumnValidator::Structure)) {
                    def.validator = Some(ColumnValidator::Basic(ColumnDataType::String));
                    def.data_type = ColumnDataType::String;
                }
                collected_defs.push(def);
                value_sources.push((src, true));
            } else if let Some(src_col) = columns_snapshot.get(src) {
                collected_defs.push(StructureFieldDefinition::from(src_col));
                value_sources.push((src, false));
            }
        }
    }

    // Create metadata with id and parent_key columns, plus schema fields
    // NOTE: These will be filtered out by the reader/writer since they're technical columns
    let mut structure_columns = vec![
        ColumnDefinition {
            header: "id".to_string(),
            data_type: ColumnDataType::String,
            validator: None,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: Some(false),
            deleted: false,
            hidden: false, // Will be filtered during read/write anyway
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        },
        ColumnDefinition {
            header: "parent_key".to_string(),
            data_type: ColumnDataType::String,
            validator: None,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            // Keep sending Parent_key by default for AI context/merge
            ai_include_in_send: Some(true),
            deleted: false,
            hidden: false, // Will be filtered during read/write anyway
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        },
    ];

    // Add columns from the structure schema
    for field_def in &collected_defs {
        structure_columns.push(ColumnDefinition {
            header: field_def.header.clone(),
            data_type: field_def.data_type,
            validator: field_def.validator.clone(),
            filter: None,
            ai_context: field_def.ai_context.clone(),
            ai_enable_row_generation: field_def.ai_enable_row_generation,
            ai_include_in_send: field_def.ai_include_in_send,
            deleted: false,
            hidden: false, // User-defined structure field
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        });
    }

    Some((collected_defs, value_sources, structure_columns))
}

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
pub fn handle_structure_conversion_from(
    grid: &mut [Vec<String>],
    col_index: usize,
    column_header: &str,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
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
