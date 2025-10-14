// src/sheets/database/writer/updates.rs
// Update operations - modifying cell values and structure data

use super::super::error::DbResult;
use rusqlite::{params, Connection};

/// Update a single cell
pub fn update_cell(
    conn: &Connection,
    table_name: &str,
    row_index: usize,
    column_name: &str,
    value: &str,
) -> DbResult<()> {
    conn.execute(
        &format!(
            "UPDATE \"{}\" SET \"{}\" = ? WHERE row_index = ?",
            table_name, column_name
        ),
        params![value, row_index as i32],
    )?;
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
    conn.execute(
        &format!(
            "UPDATE \"{}\" SET \"{}\" = ? WHERE id = ?",
            table_name, column_name
        ),
        params![value, row_id],
    )?;
    Ok(())
}

/// Update the order (column_index) for columns in the table's metadata table.
/// Pairs are (column_name, new_index). This updates metadata only; no physical reorder of table columns.
pub fn update_column_indices(
    conn: &Connection,
    table_name: &str,
    ordered_pairs: &[(String, i32)],
) -> DbResult<()> {
    let meta_table = format!("{}_Metadata", table_name);
    let tx = conn.unchecked_transaction()?;
    // Phase 1: Shift all indices by a large offset to avoid UNIQUE collisions during remap
    tx.execute(
        &format!(
            "UPDATE \"{}\" SET column_index = column_index + 10000",
            meta_table
        ),
        [],
    )?;

    // Phase 2: Apply final indices
    {
        let mut stmt = tx.prepare(&format!(
            "UPDATE \"{}\" SET column_index = ? WHERE column_name = ?",
            meta_table
        ))?;
        for (name, idx) in ordered_pairs {
            stmt.execute(params![idx, name])?;
        }
    }

    tx.commit()?;
    Ok(())
}
