// src/sheets/database/schema/helpers.rs

use crate::sheets::definitions::ColumnDataType;
use rusqlite::{params, Connection, OptionalExtension};
use super::super::error::DbResult;

/// Technical columns that are system-managed and not part of user data
pub const TECHNICAL_COLUMNS: &[&str] = &["row_index", "parent_key", "id"];

/// Check if a column name is a technical (system-managed) column
pub fn is_technical_column(column_name: &str) -> bool {
    TECHNICAL_COLUMNS.contains(&column_name)
}

/// Convert runtime column index to the actual column_index value in the metadata table.
/// 
/// IMPORTANT: The runtime_column_index is the visual position (including technical columns),
/// but we need to find the actual `column_index` value in the metadata table, which is stable
/// regardless of column order, deletions, etc.
/// 
/// This function:
/// 1. Reads the current metadata to get the column name at the runtime position
/// 2. Looks up that column's `column_index` value in the metadata table by name
/// 3. Returns that `column_index` value (or None for technical columns)
pub fn runtime_to_persisted_column_index(
    conn: &Connection,
    table_name: &str,
    runtime_column_index: usize,
    daemon_client: &super::super::daemon_client::DaemonClient,
) -> DbResult<Option<i32>> {
    // Read the full metadata to get the visual column list
    let metadata = crate::sheets::database::reader::DbReader::read_metadata(conn, table_name, daemon_client, None)?;
    
    // Get the column at the runtime index
    let Some(column) = metadata.columns.get(runtime_column_index) else {
        bevy::log::warn!(
            "Runtime column index {} out of bounds for table '{}' (has {} columns)",
            runtime_column_index,
            table_name,
            metadata.columns.len()
        );
        return Ok(None);
    };
    
    // Check if this is a technical column (row_index, parent_key, etc.)
    let column_name = &column.header;
    if is_technical_column(column_name) {
        bevy::log::debug!(
            "Skipping metadata update for technical column '{}' at runtime index {} in table '{}'",
            column_name,
            runtime_column_index,
            table_name
        );
        return Ok(None);
    }
    
    // Look up the actual column_index value in the metadata table by column name
    let meta_table = format!("{}_Metadata", table_name);
    let column_index: Option<i32> = conn
        .query_row(
            &format!("SELECT column_index FROM \"{}\" WHERE column_name = ? AND (deleted IS NULL OR deleted = 0)", meta_table),
            params![column_name],
            |row| row.get(0),
        )
        .optional()?;
    
    if column_index.is_none() {
        bevy::log::warn!(
            "Column '{}' at runtime index {} not found in metadata table '{}' for table '{}'",
            column_name,
            runtime_column_index,
            meta_table,
            table_name
        );
    }
    
    Ok(column_index)
}

/// SQL type mapping for column data types (ColumnDataType → SQL type string)
pub fn sql_type_for_column(data_type: ColumnDataType) -> &'static str {
    match data_type {
        ColumnDataType::String => "TEXT",
        ColumnDataType::Bool => "INTEGER",
        ColumnDataType::I64 => "INTEGER",
        ColumnDataType::F64 => "REAL",
    }
}

/// Convert SQL type string to ColumnDataType (inverse of sql_type_for_column)
/// Used when reading physical table schema from SQLite
pub fn sql_type_to_column_data_type(sql_type: &str) -> ColumnDataType {
    match sql_type.to_uppercase().as_str() {
        "INTEGER" => ColumnDataType::I64,
        "REAL" | "FLOAT" | "DOUBLE" => ColumnDataType::F64,
        _ => ColumnDataType::String,
    }
}

/// Convert metadata type string to ColumnDataType
/// Used when reading column definitions from metadata tables
pub fn metadata_type_to_column_data_type(type_str: &str) -> ColumnDataType {
    match type_str {
        "String" => ColumnDataType::String,
        "Bool" => ColumnDataType::Bool,
        "I64" => ColumnDataType::I64,
        "F64" => ColumnDataType::F64,
        _ => ColumnDataType::String,
    }
}

/// Produce a safe SQL identifier fragment suitable for use in unquoted index names.
/// Replaces any character that is not [A-Za-z0-9_] with an underscore and collapses repeats.
pub fn sanitize_identifier(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_us = false;
    for ch in name.chars() {
        let is_ok = ch.is_ascii_alphanumeric() || ch == '_';
        if is_ok {
            out.push(ch);
            last_us = false;
        } else if !last_us {
            out.push('_');
            last_us = true;
        }
    }
    // Trim leading/trailing underscores
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "idx".to_string()
    } else {
        trimmed
    }
}
