// src/sheets/database/schema/writer.rs
// Schema write operations - ALL writes go through daemon

use super::super::error::DbResult;
use super::super::daemon_client::{DaemonClient, Statement};
use rusqlite::Connection;
use super::queries::column_exists;

/// Add a column to a table if it doesn't exist
/// ARCHITECTURE: Uses daemon for write operation
pub fn add_column_if_missing(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    column_type: &str,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    if !column_exists(conn, table_name, column_name)? {
        let stmt = Statement {
            sql: format!(
                "ALTER TABLE \"{}\" ADD COLUMN {} {}",
                table_name, column_name, column_type
            ),
            params: vec![],
        };
        
        daemon_client.exec_batch(vec![stmt])
            .map_err(|e| super::super::error::DbError::Other(e))?;
        bevy::log::info!("Added column '{}' to table '{}'", column_name, table_name);
    }
    Ok(())
}

/// Create the global _Metadata table
/// ARCHITECTURE: Uses daemon for write operation
pub fn create_global_metadata_table(daemon_client: &DaemonClient) -> DbResult<()> {
    let stmt = Statement {
        sql: "CREATE TABLE IF NOT EXISTS _Metadata (
            table_name TEXT PRIMARY KEY,
            table_type TEXT DEFAULT 'main',
            parent_table TEXT,
            parent_column TEXT,
            ai_allow_add_rows INTEGER DEFAULT 0,
            ai_table_context TEXT,
            ai_model_id TEXT,
            ai_grounding_with_google_search INTEGER DEFAULT 0,
            ai_active_group TEXT,
            display_order INTEGER,
            category TEXT,
            hidden INTEGER DEFAULT 0,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )".to_string(),
        params: vec![],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Create main data table
/// ARCHITECTURE: Uses daemon for write operation
pub fn create_main_data_table(
    table_name: &str,
    column_defs: &[String],
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
        table_name,
        column_defs.join(", ")
    );
    
    // Create index (use sanitized name to avoid quoting issues)
    let index_name = super::sanitize_identifier(table_name);
    let index_sql = format!(
        "CREATE INDEX IF NOT EXISTS idx_{}_row_index ON \"{}\"(row_index)",
        index_name, table_name
    );
    
    let stmts = vec![
        Statement { sql: create_sql, params: vec![] },
        Statement { sql: index_sql, params: vec![] },
    ];
    
    daemon_client.exec_batch(stmts)
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Create metadata table for a sheet
/// ARCHITECTURE: Uses daemon for write operation
pub fn create_sheet_metadata_table(
    meta_table: &str,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let stmt = Statement {
        sql: format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                column_index INTEGER UNIQUE NOT NULL,
                column_name TEXT NOT NULL UNIQUE,
                display_name TEXT,
                data_type TEXT NOT NULL,
                validator_type TEXT,
                validator_config TEXT,
                ai_context TEXT,
                filter_expr TEXT,
                ai_enable_row_generation INTEGER DEFAULT 0,
                ai_include_in_send INTEGER DEFAULT 1,
                deleted INTEGER DEFAULT 0
            )",
            meta_table
        ),
        params: vec![],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Insert a single column metadata row
/// ARCHITECTURE: Uses daemon for write operation
pub fn insert_column_metadata(
    meta_table: &str,
    column_index: i32,
    column_name: &str,
    data_type: &str,
    validator_type: Option<&str>,
    validator_config: Option<&str>,
    ai_context: Option<&str>,
    filter_expr: Option<&str>,
    ai_enable_row_generation: i32,
    ai_include_in_send: i32,
    deleted: i32,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let stmt = Statement {
        sql: format!(
            "INSERT OR REPLACE INTO \"{}\" 
             (column_index, column_name, data_type, validator_type, validator_config, ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send, deleted)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            meta_table
        ),
        params: vec![
            serde_json::json!(column_index),
            serde_json::json!(column_name),
            serde_json::json!(data_type),
            serde_json::json!(validator_type),
            serde_json::json!(validator_config),
            serde_json::json!(ai_context),
            serde_json::json!(filter_expr),
            serde_json::json!(ai_enable_row_generation),
            serde_json::json!(ai_include_in_send),
            serde_json::json!(deleted),
        ],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Insert or ignore column metadata by column_index
/// ARCHITECTURE: Uses daemon for write operation
pub fn insert_column_metadata_if_missing(
    meta_table: &str,
    column_index: i32,
    column_name: &str,
    data_type: &str,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let stmt = Statement {
        sql: format!(
            "INSERT OR IGNORE INTO \"{}\" (column_index, column_name, data_type) VALUES (?, ?, ?)",
            meta_table
        ),
        params: vec![
            serde_json::json!(column_index),
            serde_json::json!(column_name),
            serde_json::json!(data_type),
        ],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Create AI groups table for a sheet
/// ARCHITECTURE: Uses daemon for write operation
pub fn create_ai_groups_table(
    groups_table: &str,
    meta_table: &str,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let stmt = Statement {
        sql: format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                column_id INTEGER NOT NULL,
                group_name TEXT NOT NULL,
                is_enabled INTEGER DEFAULT 0,
                FOREIGN KEY (column_id) REFERENCES \"{}\"(id) ON DELETE CASCADE,
                UNIQUE(column_id, group_name)
            )",
            groups_table, meta_table
        ),
        params: vec![],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Insert AI group membership
/// ARCHITECTURE: Uses daemon for write operation
pub fn insert_ai_group_column(
    groups_table: &str,
    meta_table: &str,
    group_name: &str,
    column_index: i32,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let stmt = Statement {
        sql: format!(
            "INSERT OR IGNORE INTO \"{}\" (column_id, group_name, is_enabled)
             SELECT id, ?, 1 FROM \"{}\" WHERE column_index = ?",
            groups_table, meta_table
        ),
        params: vec![
            serde_json::json!(group_name),
            serde_json::json!(column_index),
        ],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Create structure table
/// ARCHITECTURE: Uses daemon for write operation
pub fn create_structure_data_table(
    structure_table: &str,
    column_defs: &[String],
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
        structure_table,
        column_defs.join(", ")
    );
    
    bevy::log::info!("Creating structure table with SQL: {}", create_sql);
    
    // Create indexes (use sanitized name to avoid quoting issues)
    let index_name = super::sanitize_identifier(structure_table);
    let index_sql = format!(
        "CREATE INDEX IF NOT EXISTS idx_{}_parent_key ON \"{}\"(parent_key)",
        index_name, structure_table
    );
    
    let stmts = vec![
        Statement { sql: create_sql, params: vec![] },
        Statement { sql: index_sql, params: vec![] },
    ];
    
    daemon_client.exec_batch(stmts)
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Drop a table if it exists
/// ARCHITECTURE: Uses daemon for write operation
pub fn drop_table(
    table_name: &str,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let stmt = Statement {
        sql: format!("DROP TABLE IF EXISTS \"{}\"", table_name),
        params: vec![],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Register structure table in global metadata
/// ARCHITECTURE: Uses daemon for write operation
pub fn register_structure_table(
    structure_table: &str,
    parent_table: &str,
    parent_column: &str,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let stmt = Statement {
        sql: "INSERT OR REPLACE INTO _Metadata (table_name, table_type, parent_table, parent_column, hidden)
              VALUES (?, 'structure', ?, ?, 1)".to_string(),
        params: vec![
            serde_json::json!(structure_table),
            serde_json::json!(parent_table),
            serde_json::json!(parent_column),
        ],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Insert or replace table-level metadata in global _Metadata
/// ARCHITECTURE: Uses daemon for write operation
pub fn upsert_table_metadata(
    table_name: &str,
    ai_allow_add_rows: i32,
    ai_table_context: Option<&str>,
    ai_active_group: Option<&str>,
    category: Option<&str>,
    display_order: Option<i32>,
    hidden: i32,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let stmt = Statement {
        sql: "INSERT OR REPLACE INTO _Metadata 
              (table_name, table_type, ai_allow_add_rows, ai_table_context, ai_active_group, category, display_order, hidden)
              VALUES (?, 'main', ?, ?, ?, ?, ?, ?)".to_string(),
        params: vec![
            serde_json::json!(table_name),
            serde_json::json!(ai_allow_add_rows),
            serde_json::json!(ai_table_context),
            serde_json::json!(ai_active_group),
            serde_json::json!(category),
            serde_json::json!(display_order),
            serde_json::json!(hidden),
        ],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}

/// Update a specific row in a table
/// ARCHITECTURE: Uses daemon for write operation
pub fn update_table_metadata_hidden(
    condition: &str,
    daemon_client: &DaemonClient,
) -> DbResult<()> {
    let stmt = Statement {
        sql: format!("UPDATE _Metadata SET hidden = 1 WHERE {}", condition),
        params: vec![],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| super::super::error::DbError::Other(e))?;
    Ok(())
}
