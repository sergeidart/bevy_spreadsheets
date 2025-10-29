// src/sheets/database/writer/updates.rs
// Update operations - modifying cell values and structure data

use super::super::error::DbResult;
use super::helpers::{build_update_sql, metadata_table_name};
use rusqlite::{params, Connection};

/// Update a single cell
pub fn update_cell(
    conn: &Connection,
    table_name: &str,
    row_index: usize,
    column_name: &str,
    value: &str,
) -> DbResult<()> {
    let sql = build_update_sql(table_name, column_name, "row_index = ?");
    conn.execute(&sql, params![value, row_index as i32])?;
    Ok(())
}

/// Update a structure sheet's cell value by row id.
pub fn update_structure_cell_by_id(
    conn: &Connection,
    table_name: &str,
    row_id: i64,
    column_name: &str,
    value: &str,
) -> DbResult<()> {
    let sql = build_update_sql(table_name, column_name, "id = ?");
    conn.execute(&sql, params![value, row_id])?;
    Ok(())
}

/// Update the order (column_index) for columns in the table's metadata table.
/// Pairs are (column_name, new_index). This updates metadata only; no physical reorder of table columns.
pub fn update_column_indices(
    conn: &Connection,
    table_name: &str,
    ordered_pairs: &[(String, i32)],
) -> DbResult<()> {
    let meta_table = metadata_table_name(table_name);

    bevy::log::info!(
        "update_column_indices: Starting update for table '{}' with {} pairs",
        table_name, ordered_pairs.len()
    );

    let tx = conn.unchecked_transaction()?;

    // Phase 0: Move deleted columns to negative indices to get them out of the way
    bevy::log::debug!("Phase 0: Moving deleted columns to negative indices");
    let deleted_moved = tx.execute(
        &format!(
            "UPDATE \"{}\" SET column_index = -(column_index + 1) WHERE deleted = 1",
            meta_table
        ),
        [],
    )?;
    bevy::log::debug!("Phase 0: Moved {} deleted columns", deleted_moved);

    // Phase 1: Shift all non-deleted indices by a large offset to avoid UNIQUE collisions during remap
    bevy::log::debug!("Phase 1: Shifting all non-deleted column_index values by +10000");
    let shifted = tx.execute(
        &format!(
            "UPDATE \"{}\" SET column_index = column_index + 10000 WHERE (deleted IS NULL OR deleted = 0)",
            meta_table
        ),
        [],
    )?;
    bevy::log::debug!("Phase 1: Shifted {} rows", shifted);

    // Phase 2: Apply final indices
    bevy::log::debug!("Phase 2: Applying final column indices");
    {
        let mut stmt = tx.prepare(&format!(
            "UPDATE \"{}\" SET column_index = ? WHERE column_name = ?",
            meta_table
        ))?;
        for (name, idx) in ordered_pairs {
            bevy::log::trace!("Setting column '{}' to index {}", name, idx);
            let updated = stmt.execute(params![idx, name])?;
            if updated == 0 {
                bevy::log::warn!(
                    "Column '{}' not found in metadata table '{}' during reorder",
                    name, meta_table
                );
            }
        }
    }

    bevy::log::debug!("Committing transaction");
    tx.commit()?;
    bevy::log::info!("update_column_indices: Successfully updated column order for '{}'", table_name);
    Ok(())
}
