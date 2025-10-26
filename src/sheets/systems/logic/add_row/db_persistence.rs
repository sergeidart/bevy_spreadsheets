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

        // Find the parent column definition to get the structure_key_parent_column_index
        let parent_column_def = metadata.columns.iter()
            .find(|c| c.header.eq_ignore_ascii_case("parent_key"))
            .ok_or_else(|| format!("Cannot find parent_key column definition in sheet '{}'", sheet_name))?;
        
        let key_column_name = if let Some(key_col_idx) = parent_column_def.structure_key_parent_column_index {
            // Get the parent table's metadata to find column name at this index
            // We need access to registry for this, but we don't have it in this function
            // Workaround: Query the parent table's schema from the database
            let parent_table_info: Vec<(i64, String, String)> = conn.prepare(&format!("PRAGMA table_info(\"{}\")", parent_table))
                .and_then(|mut stmt| {
                    stmt.query_map([], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
                    })
                    .and_then(|mapped_rows| mapped_rows.collect::<Result<Vec<_>, _>>())
                })
                .map_err(|e| format!("Failed to query parent table schema: {}", e))?;
            
            debug!("Parent table '{}' schema: {:?}", parent_table, parent_table_info);
            
            // DB columns start with 'id', then data columns, so add 1 to skip the 'id' column
            // The structure_key_parent_column_index refers to the visual column index (0-based in metadata.columns)
            // But in the DB, we have: id, row_index (if structure), parent_key (if structure), then data columns
            // So we need to account for technical columns
            
            // Check if parent is also a structure table (has row_index and parent_key columns)
            let is_parent_structure = parent_table_info.iter()
                .any(|(_, name, _)| name.eq_ignore_ascii_case("row_index") || name.eq_ignore_ascii_case("parent_key"));
            
            let db_column_offset = if is_parent_structure {
                // Skip: id, row_index, parent_key = 3 columns
                // But key_col_idx is relative to metadata.columns which doesn't include row_index/parent_key
                // So key_col_idx + 1 (for id) + 2 (for row_index, parent_key) = key_col_idx + 3
                key_col_idx + 3
            } else {
                // Skip: id = 1 column
                key_col_idx + 1
            };
            
            parent_table_info.get(db_column_offset)
                .map(|(_, name, _)| name.clone())
                .ok_or_else(|| format!("Column index {} out of bounds in parent table '{}'", db_column_offset, parent_table))?
        } else {
            // Fallback: choose the first non-technical parent column (matches UI default)
            warn!(
                "No structure_key_parent_column_index found for parent_key in sheet '{}', using first non-technical parent column as fallback",
                sheet_name
            );

            // Query parent schema: (cid, name, type)
            let parent_table_info: Vec<(i64, String, String)> = conn
                .prepare(&format!("PRAGMA table_info(\"{}\")", parent_table))
                .and_then(|mut stmt| {
                    stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })
                    .and_then(|mapped_rows| mapped_rows.collect::<Result<Vec<_>, _>>())
                })
                .map_err(|e| format!("Failed to query parent table schema: {}", e))?;

            // Filter to first non-technical, preferably TEXT column
            let mut first_text: Option<String> = None;
            let mut first_any: Option<String> = None;
            for (_cid, name, ty) in parent_table_info.iter() {
                let lname = name.to_lowercase();
                let is_technical = lname == "id"
                    || lname == "parent_id"
                    || lname == "row_index"
                    || lname == "parent_key"
                    || lname == "created_at"
                    || lname == "updated_at"
                    || (lname.starts_with("grand_") && lname.ends_with("_parent"));
                if is_technical {
                    continue;
                }
                if first_any.is_none() {
                    first_any = Some(name.clone());
                }
                if ty.eq_ignore_ascii_case("TEXT") && first_text.is_none() {
                    first_text = Some(name.clone());
                }
            }
            first_text
                .or(first_any)
                .ok_or_else(|| format!("Cannot determine a non-technical key column in parent table '{}'", parent_table))?
        };

        debug!("Using column '{}' to validate parent_key='{}' exists in table '{}'", key_column_name, parent_key, parent_table);

        // Lookup parent_id (the 'id' field from the parent table) using parent_key
        let parent_id: i64 = conn.query_row(
            &format!("SELECT id FROM \"{}\" WHERE \"{}\" = ? LIMIT 1", parent_table, key_column_name),
            [&parent_key],
            |row| row.get(0)
        ).map_err(|e| {
            format!("Cannot find parent row with key '{}' in table '{}' (searched column '{}'). Ensure the parent row exists before adding child rows. Error: {}", 
                parent_key, parent_table, key_column_name, e)
        })?;

        debug!("Found parent row: parent_key='{}' -> parent_id={} in table '{}'", parent_key, parent_id, parent_table);

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
        
        // Add parent_key to the insert (structure tables do not have a parent_id column)
        column_names.push("parent_key".to_string());
        row_data.push(parent_key.clone());

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
        // For structure sheets, batch insert with parent_id resolution
        let first_row = grid_data.get(0).ok_or("No data to insert")?;
        let parent_key = first_row.get(1).cloned().unwrap_or_default();
        
        if parent_key.is_empty() {
            return Err(format!("Cannot insert rows into structure table '{}': parent_key is empty", sheet_name));
        }

        // Resolve parent_id using the same logic as single insert
        let parent_table = sheet_name.rsplit_once('_')
            .map(|(parent, _)| parent)
            .ok_or_else(|| format!("Cannot determine parent table from structure sheet name: {}", sheet_name))?;

        // Find the parent column definition to get the structure_key_parent_column_index
        let parent_column_def = metadata.columns.iter()
            .find(|c| c.header.eq_ignore_ascii_case("parent_key"))
            .ok_or_else(|| format!("Cannot find parent_key column definition in sheet '{}'", sheet_name))?;
        
        let key_column_name = if let Some(key_col_idx) = parent_column_def.structure_key_parent_column_index {
            // Get the parent table's metadata to find column name at this index
            let parent_table_info: Vec<(i64, String, String)> = conn.prepare(&format!("PRAGMA table_info(\"{}\")", parent_table))
                .and_then(|mut stmt| {
                    stmt.query_map([], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
                    })
                    .and_then(|mapped_rows| mapped_rows.collect::<Result<Vec<_>, _>>())
                })
                .map_err(|e| format!("Failed to query parent table schema: {}", e))?;
            
            // Check if parent is also a structure table
            let is_parent_structure = parent_table_info.iter()
                .any(|(_, name, _)| name.eq_ignore_ascii_case("row_index") || name.eq_ignore_ascii_case("parent_key"));
            
            let db_column_offset = if is_parent_structure {
                key_col_idx + 3
            } else {
                key_col_idx + 1
            };
            
            parent_table_info.get(db_column_offset)
                .map(|(_, name, _)| name.clone())
                .ok_or_else(|| format!("Column index {} out of bounds in parent table '{}'", db_column_offset, parent_table))?
        } else {
            // Fallback: choose the first non-technical parent column (matches UI default)
            warn!(
                "No structure_key_parent_column_index found for parent_key in sheet '{}' (batch), using first non-technical parent column as fallback",
                sheet_name
            );

            let parent_table_info: Vec<(i64, String, String)> = conn
                .prepare(&format!("PRAGMA table_info(\"{}\")", parent_table))
                .and_then(|mut stmt| {
                    stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })
                    .and_then(|mapped_rows| mapped_rows.collect::<Result<Vec<_>, _>>())
                })
                .map_err(|e| format!("Failed to query parent table schema: {}", e))?;

            let mut first_text: Option<String> = None;
            let mut first_any: Option<String> = None;
            for (_cid, name, ty) in parent_table_info.iter() {
                let lname = name.to_lowercase();
                let is_technical = lname == "id"
                    || lname == "parent_id"
                    || lname == "row_index"
                    || lname == "parent_key"
                    || lname == "created_at"
                    || lname == "updated_at"
                    || (lname.starts_with("grand_") && lname.ends_with("_parent"));
                if is_technical {
                    continue;
                }
                if first_any.is_none() {
                    first_any = Some(name.clone());
                }
                if ty.eq_ignore_ascii_case("TEXT") && first_text.is_none() {
                    first_text = Some(name.clone());
                }
            }
            first_text
                .or(first_any)
                .ok_or_else(|| format!("Cannot determine a non-technical key column in parent table '{}'", parent_table))?
        };

        // Lookup parent_id using the determined key column
        let parent_id: i64 = conn.query_row(
            &format!("SELECT id FROM \"{}\" WHERE \"{}\" = ? LIMIT 1", parent_table, key_column_name),
            [&parent_key],
            |row| row.get(0)
        ).map_err(|e| {
            format!("Cannot find parent row with key '{}' in table '{}' (searched column '{}'). Error: {}", 
                parent_key, parent_table, key_column_name, e)
        })?;

        debug!("Batch insert: Found parent row: parent_key='{}' -> parent_id={} in table '{}'", parent_key, parent_id, parent_table);

        // Build column names and batch data (structure tables do not have a parent_id column)
        let mut column_names: Vec<String> = Vec::new();
        let mut batch_rows: Vec<Vec<String>> = Vec::with_capacity(num_rows);
        
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
