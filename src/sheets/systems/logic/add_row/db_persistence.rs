// src/sheets/systems/logic/add_row_handlers/db_persistence.rs
// Database-specific persistence operations for add_row functionality

use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use bevy::prelude::*;

/// Prepends a row to the database table
pub(super) fn persist_row_to_db(
    metadata: &SheetMetadata,
    sheet_name: &str,
    category: &Option<String>,
    grid_data: &[Vec<String>],
) -> Result<(), String> {
    // Only proceed if this is a DB-backed sheet
    let Some(cat) = category.as_ref() else {
        return Ok(()); // Not a DB sheet, skip
    };

    let base_path = crate::sheets::systems::io::get_default_data_base_path();
    let db_path = base_path.join(format!("{}.db", cat));
    
    if !db_path.exists() {
        return Err(format!("Database file not found: {:?}", db_path));
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;

    // Build column names and row_data for DB insert, excluding the autoincrement 'id' column
    // and any Structure-valued columns (they have their own tables).
    let mut column_names: Vec<String> = Vec::new();
    let mut row_data: Vec<String> = Vec::new();
    
    // Get the first row from grid_data
    let row0 = grid_data.get(0).ok_or("No data to insert")?;
    
    // Skip index 0 if it's the auto 'id' PK
    for (i, col_def) in metadata.columns.iter().enumerate() {
        if i == 0 && col_def.header.eq_ignore_ascii_case("id") {
            // Don't attempt to insert into the autoincrement id column
            continue;
        }
        if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
            continue;
        }
        column_names.push(col_def.header.clone());
        row_data.push(row0.get(i).cloned().unwrap_or_default());
    }

    crate::sheets::database::writer::DbWriter::prepend_row(
        &conn,
        sheet_name,
        &row_data,
        &column_names,
    )
    .map_err(|e| format!("Failed to prepend row to database: {:?}", e))?;

    Ok(())
}

/// Updates AI settings in the database for a table
pub(super) fn update_table_ai_settings_db(
    category: &Option<String>,
    sheet_name: &str,
    enable_rows: Option<bool>,
) -> Result<(), String> {
    let Some(cat) = category.as_ref() else {
        return Ok(()); // Not a DB sheet
    };

    let base_path = crate::sheets::systems::io::get_default_data_base_path();
    let db_path = base_path.join(format!("{}.db", cat));
    
    if !db_path.exists() {
        return Err(format!("Database file not found: {:?}", db_path));
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;

    let _ = crate::sheets::database::schema::ensure_global_metadata_table(&conn);
    
    crate::sheets::database::writer::DbWriter::update_table_ai_settings(
        &conn,
        sheet_name,
        enable_rows,
        None,
        None,
        None,
    )
    .map_err(|e| format!("Failed to update AI settings: {:?}", e))?;

    Ok(())
}

/// Updates column AI include flag in the database
pub(super) fn update_column_ai_include_db(
    category: &Option<String>,
    sheet_name: &str,
    column_index: usize,
    include_flag: bool,
) -> Result<(), String> {
    let Some(cat) = category.as_ref() else {
        return Ok(()); // Not a DB sheet
    };

    let base_path = crate::sheets::systems::io::get_default_data_base_path();
    let db_path = base_path.join(format!("{}.db", cat));
    
    if !db_path.exists() {
        return Err(format!("Database file not found: {:?}", db_path));
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;

    let _ = crate::sheets::database::schema::ensure_global_metadata_table(&conn);
    
    crate::sheets::database::writer::DbWriter::update_column_ai_include(
        &conn,
        sheet_name,
        column_index,
        include_flag,
    )
    .map_err(|e| format!("Failed to update column AI include: {:?}", e))?;

    Ok(())
}

/// Updates column metadata in the database
pub(super) fn update_column_metadata_db(
    category: &Option<String>,
    sheet_name: &str,
    column_index: usize,
    ai_include_in_send: Option<bool>,
) -> Result<(), String> {
    let Some(cat) = category.as_ref() else {
        return Ok(()); // Not a DB sheet
    };

    let base_path = crate::sheets::systems::io::get_default_data_base_path();
    let db_path = base_path.join(format!("{}.db", cat));
    
    if !db_path.exists() {
        return Err(format!("Database file not found: {:?}", db_path));
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;

    let _ = crate::sheets::database::schema::ensure_global_metadata_table(&conn);
    
    crate::sheets::database::writer::DbWriter::update_column_metadata(
        &conn,
        sheet_name,
        column_index,
        None,
        None,
        ai_include_in_send,
    )
    .map_err(|e| format!("Failed to update column metadata: {:?}", e))?;

    Ok(())
}
