// src/sheets/systems/logic/update_cell/db_persistence.rs
//! Database persistence logic for cell updates

use crate::sheets::definitions::{ColumnValidator, SheetMetadata, StructureFieldDefinition};
use bevy::prelude::*;
use rusqlite::Connection;

/// Persists a structure table cell update to the database
pub fn persist_structure_cell_update(
    conn: &Connection,
    metadata: &SheetMetadata,
    row: &[String],
    col_idx: usize,
    col_header: &str,
    updated_value: &str,
    db_path: &std::path::Path,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) -> Result<(), String> {
    if col_idx < 2 {
        return Ok(()); // Skip id (0) and parent_key (1)
    }
    
    // Use sheet_name as table name (data_filename has .json extension which DB tables don't have)
    let table_name = &metadata.sheet_name;
    
    // Validate that the column exists before attempting to update
    use crate::sheets::database::schema::queries::column_exists;
    if !column_exists(conn, table_name, col_header)
        .map_err(|e| format!("Failed to verify column existence: {}", e))? {
        return Err(format!(
            "Structure changed: Column '{}' does not exist in table '{}'",
            col_header, table_name
        ));
    }
    
    let id_str = row.get(0).ok_or("Missing id column")?;
    let row_id = id_str.parse::<i64>()
        .map_err(|e| format!("Invalid id: {}", e))?;
    
    crate::sheets::database::writer::DbWriter::update_structure_cell_by_id(
        conn,
        table_name,
        row_id,
        col_header,
        updated_value,
        db_path.file_name().and_then(|n| n.to_str()),
        daemon_client,
    ).map_err(|e| format!("Failed to update structure cell: {}", e))?;
    
    Ok(())
}

/// Persists a regular table cell update to the database
pub fn persist_regular_cell_update(
    conn: &Connection,
    metadata: &SheetMetadata,
    row_idx: usize,
    col_header: &str,
    updated_value: &str,
    old_value: Option<&str>,
    col_idx: usize,
    category: &Option<String>,
    db_path: &std::path::Path,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) -> Result<(), String> {
    // Use sheet_name as table name (data_filename has .json extension which DB tables don't have)
    let table_name = &metadata.sheet_name;
    
    // Validate that the column exists before attempting to update
    use crate::sheets::database::schema::queries::column_exists;
    if !column_exists(conn, table_name, col_header)
        .map_err(|e| format!("Failed to verify column existence: {}", e))? {
        return Err(format!(
            "Structure changed: Column '{}' does not exist in table '{}'",
            col_header, table_name
        ));
    }
    
    // Query: SELECT id FROM table ORDER BY row_index DESC LIMIT 1 OFFSET visual_idx
    // NOTE: Reads stay direct for performance
    let row_id: i64 = conn.query_row(
        &format!(
            "SELECT id FROM \"{}\" ORDER BY row_index DESC LIMIT 1 OFFSET {}",
            table_name, row_idx
        ),
        [],
        |row| row.get(0),
    ).map_err(|e| format!("Could not find row ID for visual index {} in '{:?}/{}': {}", 
                         row_idx, category, metadata.sheet_name, e))?;
    
    // Update by ID instead of row_index - WRITE goes through daemon
    use crate::sheets::database::daemon_client::Statement;
    
    let stmt = Statement {
        sql: format!(
            "UPDATE \"{}\" SET \"{}\" = ? WHERE id = ?",
            table_name, col_header
        ),
        params: vec![
            serde_json::Value::String(updated_value.to_string()),
            serde_json::Value::Number(row_id.into()),
        ],
    };
    
    daemon_client.exec_batch(vec![stmt], db_path.file_name().and_then(|n| n.to_str()))
        .map_err(|e| format!("Failed to update cell via daemon: {}", e))?;
    
    // Check if cascade is needed (if this column is a structure key)
    if let Some(old_val) = old_value {
        super::cascade::cascade_key_change_if_needed(
            conn,
            metadata,
            col_idx,
            col_header,
            old_val,
            updated_value,
            daemon_client,
        );
    }
    
    Ok(())
}

/// Helper: normalize any JSON value to string
fn json_to_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

/// Parses JSON and expands it to rows based on schema
fn parse_structure_json_to_rows(
    json_value: &serde_json::Value,
    schema: &[StructureFieldDefinition],
) -> Vec<Vec<String>> {
    let mut rows_to_insert: Vec<Vec<String>> = Vec::new();
    
    match json_value {
        serde_json::Value::Array(arr) => {
            if arr.iter().all(|v| v.is_object()) {
                // Array of objects
                for obj in arr {
                    if let serde_json::Value::Object(m) = obj {
                        let mut row_vec: Vec<String> = Vec::with_capacity(schema.len());
                        for f in schema {
                            row_vec.push(m.get(&f.header).map(json_to_str).unwrap_or_default());
                        }
                        if row_vec.iter().any(|s| !s.trim().is_empty()) {
                            rows_to_insert.push(row_vec);
                        }
                    }
                }
            } else if arr.iter().all(|v| v.is_array()) {
                // Array of arrays
                for a in arr {
                    if let serde_json::Value::Array(inner) = a {
                        let mut row_vec: Vec<String> = inner.iter().map(json_to_str).collect();
                        if row_vec.len() < schema.len() {
                            row_vec.resize(schema.len(), String::new());
                        }
                        if row_vec.iter().any(|s| !s.trim().is_empty()) {
                            rows_to_insert.push(row_vec);
                        }
                    }
                }
            } else {
                // Array of primitives -> map by position
                let mut row_vec: Vec<String> = arr.iter().map(json_to_str).collect();
                if row_vec.len() < schema.len() {
                    row_vec.resize(schema.len(), String::new());
                }
                if row_vec.iter().any(|s| !s.trim().is_empty()) {
                    rows_to_insert.push(row_vec);
                }
            }
        }
        serde_json::Value::Object(m) => {
            // Single object
            let mut row_vec: Vec<String> = Vec::with_capacity(schema.len());
            for f in schema {
                row_vec.push(m.get(&f.header).map(json_to_str).unwrap_or_default());
            }
            if row_vec.iter().any(|s| !s.trim().is_empty()) {
                rows_to_insert.push(row_vec);
            }
        }
        _ => {}
    }
    
    rows_to_insert
}

/// Gets parent_key from a row based on column definition
fn get_parent_key_from_row(
    row: &[String],
    col_def: &crate::sheets::definitions::ColumnDefinition,
    metadata: &SheetMetadata,
) -> String {
    if let Some(kidx) = col_def.structure_key_parent_column_index {
        row.get(kidx).cloned().unwrap_or_default()
    } else {
        // Fallback: try Key, Name, ID
        let candidates = ["Key", "Name", "ID"];
        for candidate in &candidates {
            if let Some((idx, _)) = metadata.columns.iter().enumerate()
                .find(|(_, c)| c.header.eq_ignore_ascii_case(candidate)) {
                if let Some(val) = row.get(idx) {
                    return val.clone();
                }
            }
        }
        String::new()
    }
}

/// Persists a structure column JSON update to the structure table
pub fn persist_structure_json_update(
    conn: &Connection,
    sheet_name: &str,
    col_header: &str,
    row: &[String],
    metadata: &SheetMetadata,
    col_def: &crate::sheets::definitions::ColumnDefinition,
    updated_json: &str,
    db_path: &std::path::Path,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) -> Result<(), String> {
    let schema = col_def.structure_schema.as_ref()
        .ok_or("Structure column missing schema")?;
    
    let structure_table = format!("{}_{}", sheet_name, col_header);
    
    // Validate that the structure table exists and has the expected columns
    use crate::sheets::database::schema::queries::{table_exists, get_table_columns};
    
    if !table_exists(conn, &structure_table)
        .map_err(|e| format!("Failed to verify table existence: {}", e))? {
        return Err(format!(
            "Structure changed: Structure table '{}' does not exist",
            structure_table
        ));
    }
    
    // Verify critical columns exist (id, row_index, parent_key)
    let existing_cols = get_table_columns(conn, &structure_table)
        .map_err(|e| format!("Failed to get table columns: {}", e))?;
    
    for required_col in &["id", "row_index", "parent_key"] {
        if !existing_cols.iter().any(|c| c.eq_ignore_ascii_case(required_col)) {
            return Err(format!(
                "Structure changed: Required column '{}' missing from structure table '{}'",
                required_col, structure_table
            ));
        }
    }
    
    let parent_key = get_parent_key_from_row(row, col_def, metadata);
    
    // Parse JSON
    let json_value: serde_json::Value = serde_json::from_str(updated_json)
        .unwrap_or(serde_json::Value::Null);
    
    let rows_to_insert = parse_structure_json_to_rows(&json_value, schema);
    
    // Build all statements for atomic transaction through daemon
    use crate::sheets::database::daemon_client::Statement;
    let mut statements = Vec::new();
    
    // DELETE old rows
    statements.push(Statement {
        sql: format!("DELETE FROM \"{}\" WHERE parent_key = ?", structure_table),
        params: vec![serde_json::Value::String(parent_key.clone())],
    });
    
    if !rows_to_insert.is_empty() {
        // Get the maximum row_index from the entire table (READ - stays direct)
        let max_row_index: Option<i64> = conn.query_row(
            &format!("SELECT MAX(row_index) FROM \"{}\"", structure_table),
            [],
            |r| r.get(0),
        ).unwrap_or(None);
        let start_index = max_row_index.unwrap_or(-1) + 1;
        
        info!(
            "Inserting {} structure rows for parent_key='{}' in table '{}', starting at row_index={} (MAX was {:?})",
            rows_to_insert.len(), parent_key, structure_table, start_index, max_row_index
        );
        
        let field_cols = schema.iter()
            .map(|f| format!("\"{}\"", f.header))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = std::iter::repeat("?")
            .take(2 + schema.len())
            .collect::<Vec<_>>()
            .join(", ");
        let insert_sql = format!(
            "INSERT INTO \"{}\" (row_index, parent_key, {}) VALUES ({})",
            structure_table, field_cols, placeholders
        );
        
        // Build INSERT statements for daemon
        for (sidx, srow) in rows_to_insert.iter().enumerate() {
            let mut padded = srow.clone();
            if padded.len() < schema.len() {
                padded.resize(schema.len(), String::new());
            }
            
            let mut params: Vec<serde_json::Value> = Vec::with_capacity(2 + schema.len());
            params.push(serde_json::Value::Number((start_index + (sidx as i64)).into()));
            params.push(serde_json::Value::String(parent_key.clone()));
            for v in padded {
                params.push(serde_json::Value::String(v));
            }
            
            statements.push(Statement {
                sql: insert_sql.clone(),
                params,
            });
        }
    }
    
    // Execute all statements atomically through daemon
    daemon_client.exec_batch(statements, db_path.file_name().and_then(|n| n.to_str()))
        .map_err(|e| format!("Failed to execute structure update via daemon: {}", e))?;
    
    Ok(())
}

/// Main database persistence dispatcher
pub fn persist_cell_to_database(
    metadata: &SheetMetadata,
    sheet_name: &str,
    category: &Option<String>,
    row: &[String],
    row_idx: usize,
    col_idx: usize,
    col_header: &str,
    updated_value: &str,
    old_value: Option<&str>,
    is_structure_col: bool,
    looks_like_real_structure: bool,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) -> Result<(), String> {
    let cat = metadata.category.as_ref().ok_or("No category")?;
    let base = crate::sheets::systems::io::get_default_data_base_path();
    let db_path = base.join(format!("{}.db", cat));
    
    if !db_path.exists() {
        return Ok(()); // No database, skip persistence
    }
    
    let conn = crate::sheets::database::connection::DbConnection::open_existing(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;
    
    if looks_like_real_structure {
        persist_structure_cell_update(&conn, metadata, row, col_idx, col_header, updated_value, &db_path, daemon_client)?;
    } else if !is_structure_col {
        persist_regular_cell_update(
            &conn,
            metadata,
            row_idx,
            col_header,
            updated_value,
            old_value,
            col_idx,
            category,
            &db_path,
            daemon_client,
        )?;
    } else {
        // Structure JSON column update
        if let Some(col_def) = metadata.columns.get(col_idx) {
            if col_def.validator == Some(ColumnValidator::Structure) {
                persist_structure_json_update(
                    &conn,
                    sheet_name,
                    col_header,
                    row,
                    metadata,
                    col_def,
                    updated_value,
                    &db_path,
                    daemon_client,
                )?;
            }
        }
    }
    
    Ok(())
}
