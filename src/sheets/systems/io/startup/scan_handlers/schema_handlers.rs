// src/sheets/systems/io/startup/scan_handlers/schema_handlers.rs
//! Handlers for schema inference from SQLite tables.

use crate::sheets::definitions::{ColumnDataType, ColumnDefinition, ColumnValidator, SheetMetadata};
use bevy::prelude::*;

/// Infer schema from SQLite table structure and load data
/// Used for databases without SkylineDB metadata
pub fn infer_schema_and_load_table(
    conn: &rusqlite::Connection,
    table_name: &str,
    db_name: &str,
) -> Result<(SheetMetadata, Vec<Vec<String>>), String> {
    // Get column info from SQLite
    let pragma_query = format!("PRAGMA table_info(\"{}\")", table_name);
    let mut stmt = conn
        .prepare(&pragma_query)
        .map_err(|e| format!("Failed to get table info: {}", e))?;

    let column_info: Vec<(String, String)> = stmt
        .query_map([], |row| {
            let name: String = row.get(1)?;
            let type_str: String = row.get(2)?;
            Ok((name, type_str))
        })
        .map_err(|e| format!("Failed to query table info: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect column info: {}", e))?;

    if column_info.is_empty() {
        return Err("No columns found in table".to_string());
    }

    // Filter out internal columns (row_index, etc.)
    let columns: Vec<ColumnDefinition> = column_info
        .iter()
        .filter(|(name, _)| name != "row_index")
        .map(|(name, sqlite_type)| {
            // Map SQLite types to our data types
            let data_type = match sqlite_type.to_uppercase().as_str() {
                t if t.contains("INT") => ColumnDataType::I64,
                t if t.contains("REAL") || t.contains("FLOAT") || t.contains("DOUBLE") => {
                    ColumnDataType::F64
                }
                t if t.contains("BOOL") => ColumnDataType::Bool,
                _ => ColumnDataType::String,
            };

            ColumnDefinition {
                header: name.clone(),
                validator: Some(ColumnValidator::Basic(data_type)),
                data_type,
                filter: None,
                ai_context: None,
                ai_enable_row_generation: None,
                ai_include_in_send: None,
                deleted: false,
                hidden: false, // SQLite schema import: visible by default
                width: None,
                structure_schema: None,
                structure_column_order: None,
                structure_key_parent_column_index: None,
                structure_ancestor_key_parent_column_indices: None,
            }
        })
        .collect();

    // Create generic metadata
    let metadata = SheetMetadata {
        sheet_name: table_name.to_string(),
        category: Some(db_name.to_string()),
        data_filename: format!("{}.json", table_name),
        columns: columns.clone(),
        ai_general_rule: None,
        ai_model_id: "gemini-flash-latest".to_string(),
        ai_temperature: None,
        requested_grounding_with_google_search: Some(false),
        ai_enable_row_generation: false,
        ai_schema_groups: Vec::new(),
        ai_active_schema_group: None,
        random_picker: None,
        structure_parent: None,
        hidden: false,
    };

    // Load grid data
    let column_names: Vec<String> = columns.iter().map(|c| c.header.clone()).collect();
    let select_cols = column_names
        .iter()
        .map(|name| format!("\"{}\"", name))
        .collect::<Vec<_>>()
        .join(", ");

    let query = format!("SELECT {} FROM \"{}\"", select_cols, table_name);
    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| format!("Failed to prepare SELECT query: {}", e))?;

    let col_count = column_names.len();
    let rows = stmt
        .query_map([], |row| {
            let mut cells = Vec::new();
            for i in 0..col_count {
                // Try to get as string, fallback to empty
                let value: Option<String> = row.get(i).ok();
                cells.push(value.unwrap_or_default());
            }
            Ok(cells)
        })
        .map_err(|e| format!("Failed to query rows: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect rows: {}", e))?;

    info!(
        "Infer Schema: Table '{}': loaded {} rows with {} columns",
        table_name,
        rows.len(),
        col_count
    );

    Ok((metadata, rows))
}
