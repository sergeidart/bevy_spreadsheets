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

    // Get the first row from grid_data
    let row0 = grid_data.get(0).ok_or("No data to insert")?;

    // Detect if this is a structure sheet (has id and parent_key columns at indices 0 and 1)
    let is_structure_sheet = metadata.columns.len() >= 2
        && metadata.columns.get(0).map(|c| c.header.eq_ignore_ascii_case("id")).unwrap_or(false)
        && metadata.columns.get(1).map(|c| c.header.eq_ignore_ascii_case("parent_key")).unwrap_or(false);

    if is_structure_sheet {
        // For structure sheets, we need to resolve parent_id from parent_key
        let parent_key = row0.get(1).cloned().unwrap_or_default();
        
        debug!("Adding row to structure table '{}' with parent_key='{}'", sheet_name, parent_key);
        
        if parent_key.is_empty() {
            return Err(format!("Cannot insert row into structure table '{}': parent_key is empty", sheet_name));
        }

        // Extract parent table name from structure table name (format: ParentTable_ColumnName)
        let parent_table = sheet_name.rsplit_once('_')
            .map(|(parent, _)| parent)
            .ok_or_else(|| format!("Cannot determine parent table from structure sheet name: {}", sheet_name))?;

        debug!("Resolved parent table: '{}' for structure sheet '{}'", parent_table, sheet_name);

        // Try to find parent_id using the parent_key
        // First, try to find a "Key" column, then "Name", then "ID" as fallback
        let parent_id: i64 = {
            let mut id_opt: Option<i64> = None;
            
            // Try Key column first
            id_opt = conn.query_row(
                &format!("SELECT id FROM \"{}\" WHERE \"Key\" = ? LIMIT 1", parent_table),
                [&parent_key],
                |r| r.get(0)
            ).ok();
            
            // Try Name column
            if id_opt.is_none() {
                id_opt = conn.query_row(
                    &format!("SELECT id FROM \"{}\" WHERE \"Name\" = ? LIMIT 1", parent_table),
                    [&parent_key],
                    |r| r.get(0)
                ).ok();
            }
            
            // Try ID column as fallback
            if id_opt.is_none() {
                id_opt = conn.query_row(
                    &format!("SELECT id FROM \"{}\" WHERE \"ID\" = ? LIMIT 1", parent_table),
                    [&parent_key],
                    |r| r.get(0)
                ).ok();
            }
            
            id_opt.ok_or_else(|| {
                warn!("Failed to find parent_id for parent_key='{}' in table '{}'. Make sure the parent row exists.", parent_key, parent_table);
                format!("Cannot find parent row with key '{}' in table '{}'. Ensure the parent row exists before adding child rows.", parent_key, parent_table)
            })?
        };

        debug!("Found parent_id={} for parent_key='{}' in table '{}'", parent_id, parent_key, parent_table);

        // Build column names and row_data for structure table insert
        let mut column_names: Vec<String> = vec!["parent_id".to_string()];
        let mut row_data: Vec<String> = vec![parent_id.to_string()];
        
        // Skip id (index 0) and parent_key (index 1) as they are handled separately
        for (i, col_def) in metadata.columns.iter().enumerate() {
            if i <= 1 {
                continue; // Skip id and parent_key
            }
            if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                continue; // Skip nested structure columns
            }
            column_names.push(col_def.header.clone());
            row_data.push(row0.get(i).cloned().unwrap_or_default());
        }
        
        // Also add parent_key to the insert
        column_names.push("parent_key".to_string());
        row_data.push(parent_key);

        crate::sheets::database::writer::DbWriter::prepend_row(
            &conn,
            sheet_name,
            &row_data,
            &column_names,
        )
        .map_err(|e| format!("Failed to prepend row to structure table: {:?}", e))?;

    } else {
        // Regular table: build column names and row_data for DB insert
        let mut column_names: Vec<String> = Vec::new();
        let mut row_data: Vec<String> = Vec::new();
        
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
    }

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
