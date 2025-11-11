// src/sheets/database/writer/helpers.rs
// Helper functions for SQL generation and parameter preparation

use rusqlite::{params, Connection, OptionalExtension};
use super::super::error::DbResult;
use super::daemon_utils::daemon_error_to_rusqlite;

/// Quote a SQL identifier by wrapping it in double quotes.
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name)
}

/// Build a comma-separated list of quoted column names.
pub fn quote_column_list(columns: &[String]) -> String {
    columns
        .iter()
        .map(|name| quote_identifier(name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build a string of SQL placeholders (?, ?, ?, ...).
pub fn build_placeholders(count: usize) -> String {
    (0..count)
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build an INSERT SQL statement.
pub fn build_insert_sql(table_name: &str, columns: &[String]) -> String {
    let cols_str = quote_column_list(columns);
    let placeholders = build_placeholders(columns.len());
    
    format!(
        "INSERT INTO {} (row_index, {}) VALUES (?, {})",
        quote_identifier(table_name),
        cols_str,
        placeholders
    )
}

/// Build an UPDATE SQL statement for a single column.
pub fn build_update_sql(table_name: &str, column_name: &str, where_clause: &str) -> String {
    format!(
        "UPDATE {} SET {} = ? WHERE {}",
        quote_identifier(table_name),
        quote_identifier(column_name),
        where_clause
    )
}

/// Get the metadata table name for a given table.
pub fn metadata_table_name(table_name: &str) -> String {
    format!("{}_Metadata", table_name)
}

/// Get column index from metadata table by column name.
pub fn get_column_index_by_name(conn: &Connection, meta_table: &str, column_name: &str) -> DbResult<Option<i32>> {
    let result = conn.query_row(
        &format!("SELECT column_index FROM \"{}\" WHERE column_name = ?", meta_table),
        params![column_name],
        |row| row.get(0),
    ).optional()?;
    Ok(result)
}

/// Check if a column name conflicts with an existing column at a different index.
pub fn check_column_name_conflict(conn: &Connection, meta_table: &str, column_name: &str) -> DbResult<Option<(i32, i32)>> {
    let result = conn.query_row(
        &format!("SELECT column_index, deleted FROM \"{}\" WHERE column_name = ?", meta_table),
        params![column_name],
        |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?)),
    ).optional()?;
    Ok(result)
}

/// Delete a conflicting deleted column from metadata table.
pub fn delete_conflicting_deleted_column(_conn: &Connection, meta_table: &str, column_name: &str, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<()> {
    use crate::sheets::database::daemon_client::Statement;
    let stmt = Statement {
        sql: format!("DELETE FROM \"{}\" WHERE column_name = ? AND deleted = 1", meta_table),
        params: vec![serde_json::Value::String(column_name.to_string())],
    };
    daemon_client.exec_batch(vec![stmt], None).map_err(daemon_error_to_rusqlite)?;
    Ok(())
}

/// Handle column name conflict by checking if new_name exists at a different index.
pub fn handle_column_conflict(conn: &Connection, meta_table: &str, table_name: &str, new_name: &str, source_idx: i32, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<()> {
    if let Some((existing_idx, is_deleted)) = check_column_name_conflict(conn, meta_table, new_name)? {
        if existing_idx != source_idx {
            if is_deleted == 1 {
                bevy::log::warn!("Found deleted column '{}' at index {} in '{}' - deleting its metadata row (source index={})", new_name, existing_idx, meta_table, source_idx);
                delete_conflicting_deleted_column(conn, meta_table, new_name, daemon_client)?;
            } else {
                return Err(super::super::error::DbError::Other(format!("Column '{}' already exists at index {} in table '{}' (not deleted)", new_name, existing_idx, table_name)));
            }
        }
    }
    Ok(())
}

/// Rename a table in the database.
pub fn rename_table(_conn: &Connection, old_name: &str, new_name: &str, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<()> {
    use crate::sheets::database::daemon_client::Statement;
    let stmt = Statement {
        sql: format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", old_name, new_name),
        params: vec![],
    };
    daemon_client.exec_batch(vec![stmt], None).map_err(daemon_error_to_rusqlite)?;
    Ok(())
}

/// Rename a column in a table.
pub fn rename_column(_conn: &Connection, table_name: &str, old_name: &str, new_name: &str, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<()> {
    use crate::sheets::database::daemon_client::Statement;
    let stmt = Statement {
        sql: format!("ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"", table_name, old_name, new_name),
        params: vec![],
    };
    daemon_client.exec_batch(vec![stmt], None).map_err(daemon_error_to_rusqlite)?;
    Ok(())
}

/// Update column_name in metadata table by column_index.
pub fn update_metadata_column_name_by_index(_conn: &Connection, meta_table: &str, column_index: i32, new_name: &str, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<usize> {
    use crate::sheets::database::daemon_client::Statement;
    let stmt = Statement {
        sql: format!("UPDATE \"{}\" SET column_name = ? WHERE column_index = ?", meta_table),
        params: vec![
            serde_json::Value::String(new_name.to_string()),
            serde_json::Value::Number(column_index.into()),
        ],
    };
    let response = daemon_client.exec_batch(vec![stmt], None).map_err(daemon_error_to_rusqlite)?;
    Ok(response.rows_affected.unwrap_or(0))
}

/// Validate that a physical column rename succeeded by checking the table schema.
pub fn validate_physical_rename(conn: &Connection, table_name: &str, old_name: &str, new_name: &str) -> DbResult<()> {
    use super::super::reader::queries::get_physical_column_names;
    let physical_cols = get_physical_column_names(conn, table_name)?;
    if !physical_cols.iter().any(|col| col.eq_ignore_ascii_case(new_name)) {
        bevy::log::error!("validate_physical_rename: Validation failed - column '{}' not found after rename in table '{}'", new_name, table_name);
        return Err(super::super::error::DbError::Other(format!("Physical column rename validation failed: '{}' not found in table '{}'", new_name, table_name)));
    }
    if physical_cols.iter().any(|col| col.eq_ignore_ascii_case(old_name)) {
        bevy::log::warn!("validate_physical_rename: Old column '{}' still exists after rename in table '{}' - possible SQLite issue", old_name, table_name);
    }
    bevy::log::debug!("validate_physical_rename: Validation passed - column '{}' exists in table '{}'", new_name, table_name);
    Ok(())
}

/// Drop a column from a table, with fallback handling for older SQLite versions.
pub fn drop_column_with_fallback(_conn: &Connection, table_name: &str, column_name: &str, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<()> {
    use crate::sheets::database::daemon_client::Statement;
    bevy::log::info!("drop_column_with_fallback: attempting to drop '{}.{}'", table_name, column_name);
    let update_stmt = Statement {
        sql: format!("UPDATE \"{}\" SET \"{}\" = NULL", table_name, column_name),
        params: vec![],
    };
    let _ = daemon_client.exec_batch(vec![update_stmt], None);
    let drop_stmt = Statement {
        sql: format!("ALTER TABLE \"{}\" DROP COLUMN \"{}\"", table_name, column_name),
        params: vec![],
    };
    match daemon_client.exec_batch(vec![drop_stmt], None) {
        Ok(_) => bevy::log::info!("drop_column_with_fallback: successfully dropped column '{}.{}'", table_name, column_name),
        Err(e) => bevy::log::warn!("drop_column_with_fallback: failed to drop column '{}.{}': {} (column may persist with NULL values)", table_name, column_name, e),
    }
    Ok(())
}

/// Execute a function within a database transaction.
pub fn with_transaction<F>(conn: &Connection, daemon_client: &super::super::daemon_client::DaemonClient, f: F) -> DbResult<()>
where F: FnOnce(&Connection) -> DbResult<()>
{
    use crate::sheets::database::daemon_client::Statement;
    let begin_stmt = Statement { sql: "BEGIN IMMEDIATE".to_string(), params: vec![] };
    daemon_client.exec_batch(vec![begin_stmt], None).map_err(daemon_error_to_rusqlite)?;
    let result = f(conn);
    match result {
        Ok(_) => {
            let commit_stmt = Statement { sql: "COMMIT".to_string(), params: vec![] };
            daemon_client.exec_batch(vec![commit_stmt], None).map_err(daemon_error_to_rusqlite)?;
            Ok(())
        }
        Err(e) => {
            let rollback_stmt = Statement { sql: "ROLLBACK".to_string(), params: vec![] };
            let _ = daemon_client.exec_batch(vec![rollback_stmt], None);
            Err(e)
        }
    }
}

/// Rename a table triplet: data table, metadata table, and AI groups table (if present).
pub fn rename_table_triplet(conn: &Connection, old_name: &str, new_name: &str, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<()> {
    use super::super::schema::queries::table_exists;
    use crate::sheets::database::daemon_client::Statement;
    let new_data_exists = table_exists(conn, new_name)?;
    let new_meta = metadata_table_name(new_name);
    let new_meta_exists = table_exists(conn, &new_meta)?;
    if new_meta_exists && !new_data_exists {
        bevy::log::warn!("Found orphan metadata table '{}' without data table '{}'; dropping before rename.", new_meta, new_name);
        let drop_stmt = Statement { sql: format!("DROP TABLE IF EXISTS \"{}\"", new_meta), params: vec![] };
        daemon_client.exec_batch(vec![drop_stmt], None).map_err(daemon_error_to_rusqlite)?;
    }
    let new_groups = format!("{}_AIGroups", new_name);
    let new_groups_exists = table_exists(conn, &new_groups)?;
    if new_groups_exists && !new_data_exists {
        bevy::log::warn!("Found orphan AI groups table '{}' without data table '{}'; dropping before rename.", new_groups, new_name);
        let drop_stmt = Statement { sql: format!("DROP TABLE IF EXISTS \"{}\"", new_groups), params: vec![] };
        daemon_client.exec_batch(vec![drop_stmt], None).map_err(daemon_error_to_rusqlite)?;
    }
    let data_exists = table_exists(conn, old_name)?;
    if data_exists {
        rename_table(conn, old_name, new_name, daemon_client)?;
    } else {
        bevy::log::warn!("rename_table_triplet: Data table '{}' not found; skipping data rename.", old_name);
    }
    let old_meta = metadata_table_name(old_name);
    let new_meta = metadata_table_name(new_name);
    let meta_exists = table_exists(conn, &old_meta)?;
    if meta_exists {
        rename_table(conn, &old_meta, &new_meta, daemon_client)?;
    } else {
        bevy::log::warn!("rename_table_triplet: Metadata table '{}' not found; skipping metadata rename.", old_meta);
    }
    let old_groups = format!("{}_AIGroups", old_name);
    let new_groups = format!("{}_AIGroups", new_name);
    let groups_exists = table_exists(conn, &old_groups)?;
    if groups_exists {
        rename_table(conn, &old_groups, &new_groups, daemon_client)?;
    }
    let delete_stmt = Statement {
        sql: "DELETE FROM _Metadata WHERE table_name = ?".to_string(),
        params: vec![serde_json::Value::String(new_name.to_string())],
    };
    let update_stmt = Statement {
        sql: "UPDATE _Metadata SET table_name = ?, updated_at = CURRENT_TIMESTAMP WHERE table_name = ?".to_string(),
        params: vec![
            serde_json::Value::String(new_name.to_string()),
            serde_json::Value::String(old_name.to_string()),
        ],
    };
    daemon_client.exec_batch(vec![delete_stmt, update_stmt], None).map_err(daemon_error_to_rusqlite)?;
    Ok(())
}
