// src/sheets/systems/logic/update_cell/cascade.rs
//! Cascade logic for parent key value changes
//!
//! **Post-Migration Note (2025-10-27):**
//! After migrating to row_index-based references, cascades are rarely triggered.
//! Children store parent's row_index (which doesn't change on rename), so only
//! manual row_index changes (extremely rare) would trigger cascades.

use bevy::prelude::*;
use crate::sheets::definitions::ColumnValidator;

/// Checks if a column is used as a structure key by any child tables
///
/// After migration, this identifies columns that are referenced by structure_key_parent_column_index.
/// Note: With row_index references, this typically points to the row_index column (column 0),
/// so cascades are rarely triggered for user-visible data columns.
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
///
/// **Post-Migration Behavior:**
/// This is rarely called for display columns since children reference row_index (stable).
/// Only triggered when:
/// - Row_index itself changes (extremely rare, manual only)
/// - Legacy pre-migration data still using text-based keys
pub fn cascade_key_change_if_needed(
    conn: &rusqlite::Connection,
    metadata: &crate::sheets::definitions::SheetMetadata,
    sheet_name: &str,
    col_idx: usize,
    col_header: &str,
    old_value: &str,
    new_value: &str,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
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
            daemon_client,
        );
    }
}
