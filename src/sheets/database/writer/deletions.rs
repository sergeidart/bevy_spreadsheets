// src/sheets/database/writer/deletions.rs
// Deletion operations - removing rows and compacting indices

use super::super::error::DbResult;
use super::helpers::build_delete_sql;
use rusqlite::{params, Connection};

/// Delete a row
pub fn delete_row(conn: &Connection, table_name: &str, row_index: usize) -> DbResult<()> {
    let sql = build_delete_sql(table_name, "row_index = ?");
    conn.execute(&sql, params![row_index as i32])?;
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
    let sql = build_delete_sql(table_name, "row_index = ?");
    conn.execute(&sql, params![row_index as i32])?;
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
    let sql = build_delete_sql(table_name, "id = ?");
    conn.execute(&sql, params![id])?;
    Ok(())
}
