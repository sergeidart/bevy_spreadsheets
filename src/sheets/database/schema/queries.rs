// src/sheets/database/schema/queries.rs
// READ-only query operations
// 
// ARCHITECTURE NOTE: This file contains ONLY read operations.
// All write operations have been moved to schema/writer.rs and go through daemon.

use rusqlite::{Connection, OptionalExtension};
use super::super::error::{DbResult, DbError};

/// Check if a table exists in the database
pub fn table_exists(conn: &Connection, table_name: &str) -> DbResult<bool> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            [table_name],
            |row| row.get::<_, i32>(0).map(|v| v > 0),
        )
        .unwrap_or(false);
    
    // Debug: If table doesn't exist, log what tables DO exist
    if !exists {
        if let Ok(tables) = conn.query_row(
            "SELECT GROUP_CONCAT(name, ', ') FROM sqlite_master WHERE type='table'",
            [],
            |row| row.get::<_, String>(0)
        ) {
            bevy::log::debug!(
                "Table '{}' not found. Existing tables: {}",
                table_name, tables
            );
        }
    }
    
    Ok(exists)
}

/// Verify table exists or return error
pub fn require_table(conn: &Connection, table_name: &str) -> DbResult<()> {
    if !table_exists(conn, table_name)? {
        return Err(DbError::TableNotFound(table_name.to_string()));
    }
    Ok(())
}

/// Get list of existing columns in a table
pub fn get_table_columns(conn: &Connection, table_name: &str) -> DbResult<Vec<String>> {
    require_table(conn, table_name)?;
    
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns)
}

/// Check if a column exists in a table
pub fn column_exists(conn: &Connection, table_name: &str, column_name: &str) -> DbResult<bool> {
    // If table doesn't exist, column doesn't exist
    if !table_exists(conn, table_name)? {
        return Ok(false);
    }
    let columns = get_table_columns(conn, table_name)?;
    Ok(columns.iter().any(|c| c.eq_ignore_ascii_case(column_name)))
}

/// Detect if table structure changed (column count/names mismatch)
pub fn verify_table_structure(
    conn: &Connection,
    table_name: &str,
    expected_columns: &[String],
) -> DbResult<()> {
    let actual_columns = get_table_columns(conn, table_name)?;
    
    if actual_columns.len() != expected_columns.len() {
        return Err(DbError::StructureChanged(format!(
            "Table '{}' column count mismatch: expected {}, got {}",
            table_name,
            expected_columns.len(),
            actual_columns.len()
        )));
    }
    
    for (expected, actual) in expected_columns.iter().zip(actual_columns.iter()) {
        if !expected.eq_ignore_ascii_case(actual) {
            return Err(DbError::StructureChanged(format!(
                "Table '{}' column mismatch: expected '{}', got '{}'",
                table_name, expected, actual
            )));
        }
    }
    
    Ok(())
}

/// Get table type from global metadata
pub fn get_table_type(conn: &Connection, table_name: &str) -> DbResult<Option<String>> {
    let result = conn
        .query_row(
            "SELECT table_type FROM _Metadata WHERE table_name = ?",
            [table_name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(result)
}

// ============================================================================
// DEPRECATED WRITE FUNCTIONS - Moved to schema/writer.rs
// 
// The following functions have been moved to use daemon-based writes.
// Import from super::writer instead:
// 
// - add_column_if_missing → writer::add_column_if_missing
// - create_global_metadata_table → writer::create_global_metadata_table
// - create_main_data_table → writer::create_main_data_table
// - create_sheet_metadata_table → writer::create_sheet_metadata_table
// - insert_column_metadata → writer::insert_column_metadata
// - insert_column_metadata_if_missing → writer::insert_column_metadata_if_missing
// - create_ai_groups_table → writer::create_ai_groups_table
// - insert_ai_group_column → writer::insert_ai_group_column
// - create_structure_data_table → writer::create_structure_data_table
// - drop_table → writer::drop_table
// - register_structure_table → writer::register_structure_table
// - upsert_table_metadata → writer::upsert_table_metadata
// - update_table_metadata_hidden → writer::update_table_metadata_hidden
// ============================================================================
