// src/sheets/systems/logic/update_cell/cascade.rs
//! Cascade logic for parent key value changes

use bevy::prelude::*;
use crate::sheets::definitions::ColumnValidator;

/// Checks if a column is used as a structure key by any child tables
pub fn is_key_column(
    metadata: &crate::sheets::definitions::SheetMetadata,
    col_idx: usize,
) -> bool {
    metadata.columns.iter().any(|col| {
        if let Some(ColumnValidator::Structure) = &col.validator {
            if let Some(key_idx) = col.structure_key_parent_column_index {
                return key_idx == col_idx;
            }
        }
        false
    })
}

/// Triggers cascade update when a key column value changes
pub fn cascade_key_change_if_needed(
    conn: &rusqlite::Connection,
    metadata: &crate::sheets::definitions::SheetMetadata,
    sheet_name: &str,
    col_idx: usize,
    col_header: &str,
    old_value: &str,
    new_value: &str,
) {
    if is_key_column(metadata, col_idx) && old_value != new_value {
        info!(
            "Key column '{}' changed from '{}' to '{}' in table '{}'. Cascading to children...",
            col_header, old_value, new_value, sheet_name
        );
        let _ = crate::sheets::database::writer::DbWriter::cascade_key_value_change_to_children(
            conn,
            sheet_name,
            col_header,
            old_value,
            new_value,
        );
    }
}
