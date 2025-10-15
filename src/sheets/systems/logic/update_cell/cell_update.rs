// src/sheets/systems/logic/update_cell/cell_update.rs
//! Core cell value update logic

use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use bevy::prelude::*;

/// Metadata about the column being updated
pub struct ColumnUpdateMetadata {
    pub header: String,
    pub is_structure_col: bool,
    pub looks_like_real_structure: bool,
}

/// Extracts column metadata for the update operation
pub fn extract_column_metadata(
    metadata: &Option<SheetMetadata>,
    col_idx: usize,
) -> ColumnUpdateMetadata {
    if let Some(meta) = metadata {
        let header = meta
            .columns
            .get(col_idx)
            .map(|c| c.header.clone())
            .unwrap_or_default();
        
        let is_structure_col = meta
            .columns
            .get(col_idx)
            .map(|c| matches!(c.validator, Some(ColumnValidator::Structure)))
            .unwrap_or(false);
        
        let looks_like_real_structure = meta.columns.len() >= 2
            && meta
                .columns
                .get(0)
                .map(|c| c.header.eq_ignore_ascii_case("id"))
                .unwrap_or(false)
            && meta
                .columns
                .get(1)
                .map(|c| c.header.eq_ignore_ascii_case("parent_key"))
                .unwrap_or(false);
        
        ColumnUpdateMetadata {
            header,
            is_structure_col,
            looks_like_real_structure,
        }
    } else {
        ColumnUpdateMetadata {
            header: String::new(),
            is_structure_col: false,
            looks_like_real_structure: false,
        }
    }
}

/// Result of updating a cell value
pub struct CellUpdateResult {
    pub changed: bool,
    pub old_value: Option<String>,
    pub final_value: Option<String>,
}

/// Updates a cell value with structure column normalization
pub fn update_cell_value(
    cell: &mut String,
    new_value: &str,
    metadata: &Option<SheetMetadata>,
    col_idx: usize,
    row_idx: usize,
    category: &Option<String>,
    sheet_name: &str,
) -> CellUpdateResult {
    if *cell == new_value {
        trace!("Cell value unchanged for '{:?}/{}' cell[{},{}]. Skipping update.", category, sheet_name, row_idx, col_idx);
        return CellUpdateResult {
            changed: false,
            old_value: None,
            final_value: None,
        };
    }
    
    let old_value = cell.clone();
    let mut final_val = new_value.to_string();
    
    // Normalize if structure column: wrap single object into array, ensure array of objects, remove legacy linkage keys
    if let Some(meta) = metadata {
        if let Some(col_def) = meta.columns.get(col_idx) {
            if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&final_val) {
                    use serde_json::{Map, Value};
                    let mut arr: Vec<Value> = match parsed {
                        Value::Object(o) => vec![Value::Object(o)],
                        Value::Array(a) => a,
                        other => {
                            let mut m = Map::new();
                            m.insert("value".into(), other);
                            vec![Value::Object(m)]
                        }
                    };
                    for v in arr.iter_mut() {
                        if let Value::Object(o) = v {
                            o.remove("source_column_indices");
                        }
                    }
                    final_val = Value::Array(arr).to_string();
                } else {
                    final_val = "[]".to_string();
                }
            }
        }
    }
    
    trace!(
        "Updating cell [{},{}] in sheet '{:?}/{}' from '{}' to '{}'",
        row_idx,
        col_idx,
        category,
        sheet_name,
        cell,
        final_val
    );
    
    *cell = final_val.clone();
    
    CellUpdateResult {
        changed: true,
        old_value: Some(old_value),
        final_value: Some(final_val),
    }
}
