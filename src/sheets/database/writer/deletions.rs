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

/// Delete a row and compact subsequent row_index values so that UI row indices remain aligned.
/// This mirrors the behavior of in-memory grid removal which shifts indices down.
pub fn delete_row_and_compact(
    conn: &Connection,
    table_name: &str,
    row_index: usize,
) -> DbResult<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!("DELETE FROM \"{}\" WHERE row_index = ?", table_name),
        params![row_index as i32],
    )?;
    // Shift all rows with a greater row_index down by 1 to preserve contiguous indexing
    tx.execute(
        &format!(
            "UPDATE \"{}\" SET row_index = row_index - 1, updated_at = CURRENT_TIMESTAMP WHERE row_index > ?",
            table_name
        ),
        params![row_index as i32],
    )?;
    tx.commit()?;
    Ok(())
}

/// Delete a structure row by primary key id. Also compacts row_index for that parent to keep order stable.
pub fn delete_structure_row_by_id(
    conn: &Connection,
    table_name: &str,
    id: i64,
) -> DbResult<()> {
    // Fetch parent_id and row_index before deletion
    let mut parent_id: i64 = 0;
    let mut row_index: i32 = 0;
    let found: Result<(i64, i32), _> = conn.query_row(
        &format!(
            "SELECT parent_id, row_index FROM \"{}\" WHERE id = ?",
            table_name
        ),
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    );
    if let Ok((pid, ridx)) = found {
        parent_id = pid;
        row_index = ridx;
    } else {
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!("DELETE FROM \"{}\" WHERE id = ?", table_name),
        params![id],
    )?;
    // Compact indices for this parent scope only
    tx.execute(
        &format!(
            "UPDATE \"{}\" SET row_index = row_index - 1, updated_at = CURRENT_TIMESTAMP WHERE parent_id = ? AND row_index > ?",
            table_name
        ),
        params![parent_id, row_index],
    )?;
    tx.commit()?;
    Ok(())
}
