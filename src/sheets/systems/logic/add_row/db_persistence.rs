// src/sheets/systems/logic/add_row_handlers/db_persistence.rs
// Database-specific persistence operations for add_row functionality

use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use rusqlite::OptionalExtension;
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

    // Configure connection for better concurrency and consistency
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
    .map_err(|e| format!("Failed to configure database: {}", e))?;

    // Get the first row from grid_data
    let row0 = grid_data.get(0).ok_or("No data to insert")?;

    // Detect if this is a structure sheet (has row_index and parent_key columns at indices 0 and 1)
    let is_structure_sheet = metadata.columns.len() >= 2
        && metadata.columns.get(0).map(|c| c.header.eq_ignore_ascii_case("row_index")).unwrap_or(false)
        && metadata.columns.get(1).map(|c| c.header.eq_ignore_ascii_case("parent_key")).unwrap_or(false);

    if is_structure_sheet {
        // For structure sheets, parent_key now contains the parent's row_index (numeric value)
        // No need to resolve - just validate it's present and use it directly
        let parent_key = row0.get(1).cloned().unwrap_or_default();

        debug!("Adding row to structure table '{}' with parent_key (row_index)='{}'", sheet_name, parent_key);

        if parent_key.is_empty() {
            return Err(format!("Cannot insert row into structure table '{}': parent_key is empty", sheet_name));
        }

        // Validate that parent_key is numeric (row_index format)
        if parent_key.parse::<i64>().is_err() {
            warn!(
                "Parent_key '{}' is not numeric for structure table '{}'. This may be pre-migration data.",
                parent_key, sheet_name
            );
        }

        // Build column names and row_data for structure table insert
        let mut column_names: Vec<String> = Vec::new();
        let mut row_data: Vec<String> = Vec::new();
        
        // Skip row_index (index 0) and parent_key (index 1) as they are handled separately
        for (i, col_def) in metadata.columns.iter().enumerate() {
            if i <= 1 {
                continue; // Skip row_index and parent_key
            }
            if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                continue; // Skip nested structure columns
            }
            column_names.push(col_def.header.clone());
            row_data.push(row0.get(i).cloned().unwrap_or_default());
        }
        
        // Add parent_key to the insert
        column_names.push("parent_key".to_string());
        row_data.push(parent_key.clone());

        // Backward compatibility: some older DBs still have NOT NULL parent_id in structure tables.
        // If the physical table has parent_id, compute it from the parent table using parent_key (row_index) and include it.
        let has_parent_id: bool = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_table_info(\"{}\") WHERE name = 'parent_id'", sheet_name),
                [],
                |row| row.get::<_, i32>(0).map(|c| c > 0),
            )
            .unwrap_or(false);
        if has_parent_id {
            // Derive parent table name by stripping the last _segment from the structure sheet name
            if let Some((parent_table, _)) = sheet_name.rsplit_once('_') {
                // Look up parent id by row_index
                let parent_id: Option<i64> = conn
                    .query_row(
                        &format!("SELECT id FROM \"{}\" WHERE row_index = ?", parent_table),
                        [parent_key.as_str()],
                        |row| row.get(0),
                    )
                    .optional()
                    .unwrap_or(None);
                if let Some(pid) = parent_id {
                    column_names.push("parent_id".to_string());
                    row_data.push(pid.to_string());
                } else {
                    warn!(
                        "persist_row_to_db: parent_id not found for '{}', parent_key(row_index)='{}' in parent '{}'",
                        sheet_name, parent_key, parent_table
                    );
                }
            }
        }

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

/// Batch prepends multiple rows to the database table with single row_index calculation
pub(super) fn persist_rows_batch_to_db(
    metadata: &SheetMetadata,
    sheet_name: &str,
    category: &Option<String>,
    grid_data: &[Vec<String>],
    num_rows: usize,
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

    // Configure connection
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
    .map_err(|e| format!("Failed to configure database: {}", e))?;

    // Detect if this is a structure sheet
    let is_structure_sheet = metadata.columns.len() >= 2
        && metadata.columns.get(0).map(|c| c.header.eq_ignore_ascii_case("row_index")).unwrap_or(false)
        && metadata.columns.get(1).map(|c| c.header.eq_ignore_ascii_case("parent_key")).unwrap_or(false);

    if is_structure_sheet {
        // For structure sheets, parent_key now contains the parent's row_index (numeric value)
        let first_row = grid_data.get(0).ok_or("No data to insert")?;
        let parent_key = first_row.get(1).cloned().unwrap_or_default();

        debug!("Batch insert to structure table '{}' with parent_key (row_index)='{}'", sheet_name, parent_key);

        if parent_key.is_empty() {
            return Err(format!("Cannot insert rows into structure table '{}': parent_key is empty", sheet_name));
        }

        // Validate that parent_key is numeric (row_index format)
        if parent_key.parse::<i64>().is_err() {
            warn!(
                "Parent_key '{}' is not numeric for structure table '{}' (batch). This may be pre-migration data.",
                parent_key, sheet_name
            );
        }

        // Build column names and batch data
        let mut column_names: Vec<String> = Vec::new();
        let mut batch_rows: Vec<Vec<String>> = Vec::with_capacity(num_rows);
        // Backward compatibility: detect parent_id presence once
        let has_parent_id: bool = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_table_info(\"{}\") WHERE name = 'parent_id'", sheet_name),
                [],
                |row| row.get::<_, i32>(0).map(|c| c > 0),
            )
            .unwrap_or(false);
        let parent_table_opt = sheet_name.rsplit_once('_').map(|(p, _)| p.to_string());
        
        // Process each row in grid
        for row_idx in 0..num_rows {
            if let Some(row) = grid_data.get(row_idx) {
                let mut row_data: Vec<String> = Vec::new();
                
                // Skip row_index (0) and parent_key (1), add other columns
                for (i, col_def) in metadata.columns.iter().enumerate() {
                    if i <= 1 {
                        continue; // Skip technical columns
                    }
                    if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                        continue; // Skip nested structures
                    }
                    if row_idx == 0 {
                        column_names.push(col_def.header.clone());
                    }
                    row_data.push(row.get(i).cloned().unwrap_or_default());
                }
                
                // Add parent_key
                if row_idx == 0 {
                    column_names.push("parent_key".to_string());
                }
                row_data.push(parent_key.clone());

                // If parent_id exists, add it by resolving parent's id from row_index
                if has_parent_id {
                    if let Some(parent_table) = parent_table_opt.as_deref() {
                        let parent_id: Option<i64> = conn
                            .query_row(
                                &format!("SELECT id FROM \"{}\" WHERE row_index = ?", parent_table),
                                [parent_key.as_str()],
                                |row| row.get(0),
                            )
                            .optional()
                            .unwrap_or(None);
                        if row_idx == 0 {
                            column_names.push("parent_id".to_string());
                        }
                        row_data.push(parent_id.map(|v| v.to_string()).unwrap_or_default());
                    }
                }
                
                batch_rows.push(row_data);
            }
        }

        crate::sheets::database::writer::DbWriter::prepend_rows_batch(
            &conn,
            sheet_name,
            &batch_rows,
            &column_names,
        )
        .map_err(|e| format!("Failed to batch prepend rows to structure table: {:?}", e))?;

    } else {
        // Regular table: batch insert
        let mut column_names: Vec<String> = Vec::new();
        let mut batch_rows: Vec<Vec<String>> = Vec::with_capacity(num_rows);
        
        for row_idx in 0..num_rows {
            if let Some(row) = grid_data.get(row_idx) {
                let mut row_data: Vec<String> = Vec::new();
                
                for (i, col_def) in metadata.columns.iter().enumerate() {
                    if i == 0 && col_def.header.eq_ignore_ascii_case("id") {
                        continue; // Skip autoincrement id
                    }
                    if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                        continue;
                    }
                    if row_idx == 0 {
                        column_names.push(col_def.header.clone());
                    }
                    row_data.push(row.get(i).cloned().unwrap_or_default());
                }
                
                batch_rows.push(row_data);
            }
        }

        crate::sheets::database::writer::DbWriter::prepend_rows_batch(
            &conn,
            sheet_name,
            &batch_rows,
            &column_names,
        )
        .map_err(|e| format!("Failed to batch prepend rows to database: {:?}", e))?;
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

    // Configure connection for better concurrency
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
    .map_err(|e| format!("Failed to configure database: {}", e))?;

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

    // Configure connection for better concurrency
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
    .map_err(|e| format!("Failed to configure database: {}", e))?;

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

    // Configure connection for better concurrency
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
    .map_err(|e| format!("Failed to configure database: {}", e))?;

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
