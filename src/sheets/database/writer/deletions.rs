// src/sheets/database/writer/deletions.rs
// Deletion operations - removing rows and compacting indices

use super::super::error::DbResult;
use rusqlite::{params, Connection};

/// Delete a row
pub fn delete_row(conn: &Connection, table_name: &str, row_index: usize) -> DbResult<()> {
    conn.execute(
        &format!("DELETE FROM \"{}\" WHERE row_index = ?", table_name),
        params![row_index as i32],
    )?;
    Ok(())
}

/// Delete a row without compacting. With DESC sort order, we don't need to shift indices.
/// This is now a simple O(1) delete operation!
pub fn delete_row_and_compact(
    conn: &Connection,
    table_name: &str,
    row_index: usize,
) -> DbResult<()> {
    // Simple delete - no transaction needed, no compaction
    conn.execute(
        &format!("DELETE FROM \"{}\" WHERE row_index = ?", table_name),
        params![row_index as i32],
    )?;
    Ok(())
}

/// Delete a structure row by primary key id. No compaction needed with DESC sort.
/// Simple O(1) delete operation!
pub fn delete_structure_row_by_id(
    conn: &Connection,
    table_name: &str,
    id: i64,
) -> DbResult<()> {
    // Simple delete - no need to fetch indices or compact
    conn.execute(
        &format!("DELETE FROM \"{}\" WHERE id = ?", table_name),
        params![id],
    )?;
    Ok(())
}
