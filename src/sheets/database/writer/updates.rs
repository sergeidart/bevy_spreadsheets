// src/sheets/database/writer/updates.rs
// Update operations - modifying cell values and structure data

use super::super::error::DbResult;
use super::helpers::{build_update_sql, metadata_table_name};
use rusqlite::Connection;

/// Update a structure sheet's cell value by row id.
pub fn update_structure_cell_by_id(
    _conn: &Connection,
    table_name: &str,
    row_id: i64,
    column_name: &str,
    value: &str,
    db_filename: Option<&str>,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) -> DbResult<()> {
    // WRITE through daemon
    use crate::sheets::database::daemon_client::Statement;
    
    let sql = build_update_sql(table_name, column_name, "id = ?");
    
    let stmt = Statement {
        sql,
        params: vec![
            serde_json::Value::String(value.to_string()),
            serde_json::Value::Number(row_id.into()),
        ],
    };
    
    daemon_client.exec_batch(vec![stmt], db_filename)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e
        ))))?;
    
    Ok(())
}

/// Update the order (column_index) for columns in the table's metadata table.
/// Pairs are (column_name, new_index). This updates metadata only; no physical reorder of table columns.
pub fn update_column_indices(
    _conn: &Connection,
    table_name: &str,
    ordered_pairs: &[(String, i32)],
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) -> DbResult<()> {
    use crate::sheets::database::daemon_client::Statement;
    
    let meta_table = metadata_table_name(table_name);

    bevy::log::info!(
        "update_column_indices: Starting update for table '{}' with {} pairs",
        table_name, ordered_pairs.len()
    );

    // Phase 0: Move deleted columns to negative indices to get them out of the way
    bevy::log::debug!("Phase 0: Moving deleted columns to negative indices");
    let stmt0 = Statement {
        sql: format!(
            "UPDATE \"{}\" SET column_index = -(column_index + 1) WHERE deleted = 1",
            meta_table
        ),
        params: vec![],
    };

    // Phase 1: Shift all non-deleted indices by a large offset to avoid UNIQUE collisions during remap
    bevy::log::debug!("Phase 1: Shifting all non-deleted column_index values by +10000");
    let stmt1 = Statement {
        sql: format!(
            "UPDATE \"{}\" SET column_index = column_index + 10000 WHERE (deleted IS NULL OR deleted = 0)",
            meta_table
        ),
        params: vec![],
    };

    // Phase 2: Apply final indices
    bevy::log::debug!("Phase 2: Applying final column indices");
    let mut phase2_stmts = Vec::new();
    for (name, idx) in ordered_pairs {
        bevy::log::trace!("Setting column '{}' to index {}", name, idx);
        phase2_stmts.push(Statement {
            sql: format!(
                "UPDATE \"{}\" SET column_index = ? WHERE column_name = ?",
                meta_table
            ),
            params: vec![
                serde_json::Value::Number((*idx).into()),
                serde_json::Value::String(name.clone()),
            ],
        });
    }

    // Execute all statements as a batch
    let mut all_stmts = vec![stmt0, stmt1];
    all_stmts.extend(phase2_stmts);
    
    daemon_client.exec_batch(all_stmts, None)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e
        ))))?;

    // In test mode with mock daemon, also apply changes directly to the in-memory DB
    #[cfg(test)]
    {
        use rusqlite::params;
        // Mirror Phase 0 (ignore errors – only for tests with mock daemon)
        if let Ok(mut st0) = _conn.prepare(&format!(
            "UPDATE \"{}\" SET column_index = -(column_index + 1) WHERE deleted = 1",
            meta_table
        )) { let _ = st0.execute([]); }
        // Mirror Phase 1
        if let Ok(mut st1) = _conn.prepare(&format!(
            "UPDATE \"{}\" SET column_index = column_index + 10000 WHERE (deleted IS NULL OR deleted = 0)",
            meta_table
        )) { let _ = st1.execute([]); }
        // Mirror Phase 2 (reuse prepared statement)
        if let Ok(mut stp2) = _conn.prepare(&format!(
            "UPDATE \"{}\" SET column_index = ? WHERE column_name = ?",
            meta_table
        )) {
            for (name, idx) in ordered_pairs {
                let _ = stp2.execute(params![*idx, name]);
            }
        }
    }

    bevy::log::info!("update_column_indices: Successfully updated column order for '{}'", table_name);
    Ok(())
}
