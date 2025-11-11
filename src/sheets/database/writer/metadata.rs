// src/sheets/database/writer/metadata.rs
// Metadata operations - managing table and column metadata, AI settings

use super::super::error::DbResult;
use super::super::schema::{sql_type_for_column, runtime_to_persisted_column_index};
use super::helpers::metadata_table_name;
use crate::sheets::definitions::{ColumnDataType, ColumnValidator};
use crate::sheets::database::daemon_client::{DaemonClient, Statement};
use rusqlite::Connection;

// ============================================================================
// Helper Functions
// ============================================================================

/// Execute a daemon statement with error handling
/// Treats "no such table" errors on metadata tables as non-fatal (WAL timing issue)
fn exec_daemon_stmt(sql: String, params: Vec<serde_json::Value>, db_filename: Option<&str>, daemon_client: &DaemonClient) -> DbResult<()> {
    let stmt = Statement { sql: sql.clone(), params };
    match daemon_client.exec_batch(vec![stmt], db_filename) {
        Ok(_) => Ok(()),
        Err(e) => {
            // Check if this is a "no such table" error on a metadata table
            // This can happen when metadata table was just created via daemon but not yet visible due to WAL
            let is_metadata_table = sql.contains("_Metadata");
            let is_no_such_table = e.contains("no such table");
            
            if is_metadata_table && is_no_such_table {
                // This is expected during startup/rapid operations - table will be available on next operation
                bevy::log::debug!("Metadata table operation deferred (WAL timing): {}", e);
                Ok(()) // Treat as non-fatal
            } else {
                Err(super::super::error::DbError::Other(e))
            }
        }
    }
}

/// Get persisted index or return Ok(()) if technical column (with early return)
fn get_persisted_index_or_skip(
    conn: &Connection,
    table_name: &str,
    runtime_idx: usize,
    daemon_client: &DaemonClient,
) -> DbResult<Option<i32>> {
    match runtime_to_persisted_column_index(conn, table_name, runtime_idx, daemon_client)? {
        Some(idx) => Ok(Some(idx)),
        None => {
            bevy::log::debug!("Skipping technical column {} in '{}'", runtime_idx, table_name);
            Ok(None)
        }
    }
}

/// Convert validator to metadata tuple (type, config)
fn validator_to_metadata(
    validator: &Option<ColumnValidator>,
    table_name: &str,
    column_name: &str,
) -> (Option<String>, Option<String>) {
    match validator {
        Some(ColumnValidator::Basic(_)) => (Some("Basic".to_string()), None),
        Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
            let cfg = serde_json::json!({
                "target_table": target_sheet_name,
                "target_column_index": target_column_index
            }).to_string();
            (Some("Linked".to_string()), Some(cfg))
        }
        Some(ColumnValidator::Structure) => {
            let cfg = serde_json::json!({
                "structure_table": format!("{}_{}", table_name, column_name)
            }).to_string();
            (Some("Structure".to_string()), Some(cfg))
        }
        None => (None, None),
    }
}

/// Convert string option to JSON value, treating empty strings as NULL
fn string_to_json(s: Option<&str>) -> serde_json::Value {
    match s {
        Some(v) if !v.trim().is_empty() => serde_json::Value::String(v.to_string()),
        _ => serde_json::Value::Null,
    }
}

/// Convert bool to JSON number (0 or 1)
#[inline]
fn bool_to_json(b: bool) -> serde_json::Value {
    serde_json::Value::Number((b as i32).into())
}

/// Convert optional string to JSON value
fn opt_string_to_json(s: Option<String>) -> serde_json::Value {
    match s {
        Some(v) => serde_json::Value::String(v),
        None => serde_json::Value::Null,
    }
}

// ============================================================================
// Public API Functions
// ============================================================================

// ============================================================================
// Public API Functions
// ============================================================================

/// Update a table's hidden flag in the global _Metadata table
pub fn update_table_hidden(
    _conn: &Connection,
    table_name: &str,
    hidden: bool,
    db_filename: Option<&str>,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let sql = "INSERT INTO _Metadata (table_name, hidden) VALUES (?, ?) \
              ON CONFLICT(table_name) DO UPDATE SET hidden = excluded.hidden, updated_at = CURRENT_TIMESTAMP".to_string();
    let params = vec![
        serde_json::Value::String(table_name.to_string()),
        bool_to_json(hidden),
    ];
    exec_daemon_stmt(sql, params, db_filename, daemon_client)
}

/// Update table-level flags in _Metadata
pub fn update_table_ai_settings(
    _conn: &Connection,
    table_name: &str,
    allow_add_rows: Option<bool>,
    table_context: Option<&str>,
    model_id: Option<&str>,
    active_group: Option<&str>,
    grounding_with_google_search: Option<bool>,
    db_filename: Option<&str>,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let mut sets: Vec<&str> = Vec::new();
    let mut params: Vec<serde_json::Value> = Vec::new();
    
    if let Some(v) = allow_add_rows {
        sets.push("ai_allow_add_rows = ?");
        params.push(bool_to_json(v));
    }
    if let Some(v) = table_context {
        sets.push("ai_table_context = ?");
        params.push(serde_json::Value::String(v.to_string()));
    }
    if let Some(v) = model_id {
        sets.push("ai_model_id = ?");
        params.push(serde_json::Value::String(v.to_string()));
    }
    if let Some(v) = active_group {
        sets.push("ai_active_group = ?");
        params.push(serde_json::Value::String(v.to_string()));
    }
    if let Some(v) = grounding_with_google_search {
        sets.push("ai_grounding_with_google_search = ?");
        params.push(bool_to_json(v));
    }
    
    if sets.is_empty() {
        return Ok(());
    }
    
    let sql = format!(
        "UPDATE _Metadata SET {}, updated_at = CURRENT_TIMESTAMP WHERE table_name = ?",
        sets.join(", ")
    );
    params.push(serde_json::Value::String(table_name.to_string()));
    
    exec_daemon_stmt(sql, params, db_filename, daemon_client)
}

/// Update a column's filter, ai_context, and include flag in the table's metadata table
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_column_metadata(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    filter_expr: Option<&str>,
    ai_context: Option<&str>,
    ai_include_in_send: Option<bool>,
    db_filename: Option<&str>,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let persisted_index = match get_persisted_index_or_skip(conn, table_name, column_index, daemon_client)? {
        Some(idx) => idx,
        None => return Ok(()),
    };
    
    // Defensive: ensure metadata tables exist
    let _ = crate::sheets::database::schema::ensure_global_metadata_table(conn, daemon_client);
    let inferred_meta = match crate::sheets::database::reader::DbReader::read_metadata(conn, table_name, daemon_client) {
        Ok(m) => m,
        Err(_) => crate::sheets::definitions::SheetMetadata::create_generic(
            table_name.to_string(),
            format!("{}.json", table_name),
            (column_index + 1).max(1),
            None,
        ),
    };
    let _ = crate::sheets::database::schema::ensure_table_metadata_schema(conn, table_name, &inferred_meta, daemon_client);
    
    let mut sets: Vec<&str> = Vec::new();
    let mut params: Vec<serde_json::Value> = Vec::new();
    
    if filter_expr.is_some() {
        sets.push("filter_expr = ?");
        params.push(string_to_json(filter_expr));
    }
    if ai_context.is_some() {
        sets.push("ai_context = ?");
        params.push(string_to_json(ai_context));
    }
    if let Some(v) = ai_include_in_send {
        sets.push("ai_include_in_send = ?");
        params.push(bool_to_json(v));
    }
    
    if sets.is_empty() {
        return Ok(());
    }
    
    let meta_table = metadata_table_name(table_name);
    let sql = format!("UPDATE \"{}\" SET {} WHERE column_index = ?", meta_table, sets.join(", "));
    params.push(serde_json::Value::Number(persisted_index.into()));
    
    bevy::log::info!("update_column_metadata: runtime={} -> persisted={}", column_index, persisted_index);
    exec_daemon_stmt(sql, params, db_filename, daemon_client)
}

/// Explicitly set the AI include flag for a column in the metadata table (true = 1, false = 0)
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_column_ai_include(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    include: bool,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let persisted_index = match get_persisted_index_or_skip(conn, table_name, column_index, daemon_client)? {
        Some(idx) => idx,
        None => return Ok(()),
    };
    
    let meta_table = metadata_table_name(table_name);
    bevy::log::info!("update_column_ai_include: runtime={} -> persisted={} include={}", column_index, persisted_index, include);
    
    let sql = format!("UPDATE \"{}\" SET ai_include_in_send = ? WHERE column_index = ?", meta_table);
    let params = vec![bool_to_json(include), serde_json::Value::Number(persisted_index.into())];
    
    exec_daemon_stmt(sql, params, None, daemon_client)
}

/// Update a column's validator (data_type, validator_type, validator_config) and optional AI flags in metadata
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_column_validator(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    data_type: ColumnDataType,
    validator: &Option<ColumnValidator>,
    ai_include_in_send: Option<bool>,
    ai_enable_row_generation: Option<bool>,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let persisted_index = match get_persisted_index_or_skip(conn, table_name, column_index, daemon_client)? {
        Some(idx) => idx,
        None => return Ok(()),
    };
    
    // Defensive: ensure metadata tables exist
    let _ = crate::sheets::database::schema::ensure_global_metadata_table(conn, daemon_client);
    let inferred_meta = match crate::sheets::database::reader::DbReader::read_metadata(conn, table_name, daemon_client) {
        Ok(m) => m,
        Err(_) => crate::sheets::definitions::SheetMetadata::create_generic(
            table_name.to_string(),
            format!("{}.json", table_name),
            (column_index + 1).max(1),
            None,
        ),
    };
    let _ = crate::sheets::database::schema::ensure_table_metadata_schema(conn, table_name, &inferred_meta, daemon_client);
    
    let meta_table = metadata_table_name(table_name);
    let (validator_type, validator_config) = validator_to_metadata(validator, table_name, "");
    
    let mut sets = vec!["data_type = ?", "validator_type = ?", "validator_config = ?"];
    let mut params = vec![
        serde_json::Value::String(format!("{:?}", data_type)),
        opt_string_to_json(validator_type),
        opt_string_to_json(validator_config),
    ];
    
    if let Some(v) = ai_include_in_send {
        sets.push("ai_include_in_send = ?");
        params.push(bool_to_json(v));
    }
    if let Some(v) = ai_enable_row_generation {
        sets.push("ai_enable_row_generation = ?");
        params.push(bool_to_json(v));
    }
    
    params.push(serde_json::Value::Number((persisted_index as i32).into()));
    
    let sql = format!("UPDATE \"{}\" SET {} WHERE column_index = ?", meta_table, sets.join(", "));
    bevy::log::info!("update_column_validator: runtime={} -> persisted={}", column_index, persisted_index);
    
    exec_daemon_stmt(sql, params, None, daemon_client)
}

/// Update a column's display name (UI-only) in the table's metadata table
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_column_display_name(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    display_name: &str,
    db_filename: Option<&str>,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let persisted_index = match get_persisted_index_or_skip(conn, table_name, column_index, daemon_client)? {
        Some(idx) => idx,
        None => return Ok(()),
    };

    let meta_table = metadata_table_name(table_name);
    bevy::log::info!("update_column_display_name: runtime={} -> persisted={}", column_index, persisted_index);
    
    let sql = format!("UPDATE \"{}\" SET display_name = ? WHERE column_index = ?", meta_table);
    let params = vec![
        serde_json::Value::String(display_name.to_string()),
        serde_json::Value::Number((persisted_index as i32).into()),
    ];
    
    exec_daemon_stmt(sql, params, db_filename, daemon_client)
}

/// Add a new column to a table (main or structure) and insert its metadata row with given index.
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn add_column_with_metadata(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    data_type: ColumnDataType,
    validator: Option<ColumnValidator>,
    column_index: usize,
    ai_context: Option<&str>,
    filter_expr: Option<&str>,
    ai_enable_row_generation: Option<bool>,
    ai_include_in_send: Option<bool>,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let meta_table = metadata_table_name(table_name);
    
    // Calculate persisted index by counting existing data columns in metadata
    let persisted_index: usize = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM \"{}\" WHERE deleted IS NULL OR deleted = 0", meta_table),
            [],
            |row| row.get::<_, i32>(0).map(|v| v as usize),
        )
        .unwrap_or(0);
    
    bevy::log::info!("add_column_with_metadata: runtime={} -> persisted={} col='{}'", column_index, persisted_index, column_name);
    
    // Check if column exists physically; if not, add it
    let mut exists_stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    let mut col_exists = false;
    for row in exists_stmt.query_map([], |r| r.get::<_, String>(1))? {
        if row? == column_name {
            col_exists = true;
            break;
        }
    }
    
    if !col_exists {
        let sql_type = sql_type_for_column(data_type);
        let sql = format!("ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}", table_name, column_name, sql_type);
        exec_daemon_stmt(sql, vec![], None, daemon_client)?;
    }

    let (validator_type, validator_config) = validator_to_metadata(&validator, table_name, column_name);
    
    // Try to reuse a deleted metadata slot before inserting
    let reuse_sql = format!(
        "UPDATE \"{}\" SET column_name = ?, data_type = ?, validator_type = ?, validator_config = ?, \
         ai_context = ?, filter_expr = ?, ai_enable_row_generation = ?, ai_include_in_send = ?, deleted = 0 \
         WHERE column_index = ? AND deleted = 1",
        meta_table
    );
    
    let params = vec![
        serde_json::Value::String(column_name.to_string()),
        serde_json::Value::String(format!("{:?}", data_type)),
        opt_string_to_json(validator_type.clone()),
        opt_string_to_json(validator_config.clone()),
        string_to_json(ai_context),
        string_to_json(filter_expr),
        bool_to_json(ai_enable_row_generation.unwrap_or(false)),
        bool_to_json(ai_include_in_send.unwrap_or(true)),
        serde_json::Value::Number((persisted_index as i32).into()),
    ];
    
    let stmt = Statement { sql: reuse_sql, params: params.clone() };
    let response = daemon_client.exec_batch(vec![stmt], None)
        .map_err(|e| super::super::error::DbError::Other(e))?;
    
    if response.rows_affected.unwrap_or(0) > 0 {
        bevy::log::info!("Reused deleted metadata slot persisted={}", persisted_index);
        return Ok(());
    }
    
    // Insert new metadata row
    let insert_sql = format!(
        "INSERT OR REPLACE INTO \"{}\" (column_index, column_name, data_type, validator_type, validator_config, \
         ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        meta_table
    );
    
    exec_daemon_stmt(insert_sql, params, None, daemon_client)
}
