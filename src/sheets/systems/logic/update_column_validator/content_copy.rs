// src/sheets/systems/logic/update_column_validator/content_copy.rs
// Database content copying for structure tables

use bevy::prelude::*;
use unicode_normalization::UnicodeNormalization;

use crate::sheets::definitions::ColumnDefinition;

/// Normalize column name for comparison (same logic as UI validation)
fn normalize_column_name(s: &str) -> String {
    s.replace(['\r', '\n'], "")
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

/// Copy content from parent table to newly created structure table
/// Reads all rows from parent table and creates corresponding child rows in structure table
pub fn copy_parent_content_to_structure_table(
    conn: &rusqlite::Connection,
    parent_table_name: &str,
    structure_table_name: &str,
    parent_col_def: &ColumnDefinition,
    structure_columns: &[ColumnDefinition],
) -> Result<(), String> {
    // Use structure_columns which includes ALL columns (technical + data)
    // Filter out row_index (index 0) since it's auto-generated
    let columns_to_copy: Vec<&ColumnDefinition> = structure_columns.iter()
        .filter(|col| !col.header.eq_ignore_ascii_case("row_index"))
        .collect();
    
    info!("copy_parent_content: structure has {} total columns, {} to copy (excluding row_index)", 
        structure_columns.len(), columns_to_copy.len());
    for (i, col) in columns_to_copy.iter().enumerate() {
        info!("  column[{}]: header='{}', data_type={:?}", i, col.header, col.data_type);
    }
    
    // Find the key column index in parent table
    let key_column_index = parent_col_def
        .structure_key_parent_column_index
        .ok_or_else(|| "Structure column missing key_parent_column_index".to_string())?;
    
    info!("copy_parent_content: key_column_index={}", key_column_index);
    
    // Get all column names from parent table
    let parent_columns: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info(\"{}\")", parent_table_name))
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(1))
                .and_then(|mapped_rows| mapped_rows.collect::<Result<Vec<_>, _>>())
        })
        .map_err(|e| format!("Failed to query parent table schema: {}", e))?;
    
    info!("======================================");
    info!("PARENT TABLE ANALYSIS: {}", parent_table_name);
    info!("Parent table columns ({} total): {:?}", parent_columns.len(), parent_columns);
    
    // Check for hierarchy columns
    let has_parent_key = parent_columns.iter().any(|c| normalize_column_name(c) == normalize_column_name("parent_key"));
    let grand_columns: Vec<&String> = parent_columns.iter()
        .filter(|c| c.starts_with("grand_") && c.ends_with("_parent"))
        .collect();
    
    info!("  - Has parent_key: {}", has_parent_key);
    info!("  - Grand columns: {:?}", grand_columns);
    info!("======================================");
    
    // Find key column name (skip id, row_index)
    let _key_column_name = parent_columns.get(key_column_index + 2)
        .ok_or_else(|| format!("Key column index {} out of bounds", key_column_index))?;
    
    // Read all rows from parent table - preserve types as rusqlite::types::Value
    let query = format!("SELECT * FROM \"{}\" ORDER BY row_index", parent_table_name);
    let mut stmt = conn.prepare(&query)
        .map_err(|e| format!("Failed to prepare parent query: {}", e))?;
    
    let column_count = parent_columns.len();
    let mut parent_rows: Vec<Vec<rusqlite::types::Value>> = Vec::new();
    
    let rows = stmt.query_map([], |row| {
        let mut row_data = Vec::with_capacity(column_count);
        for i in 0..column_count {
            // Read as Value to preserve type information
            let val: rusqlite::types::Value = row.get(i).unwrap_or(rusqlite::types::Value::Null);
            row_data.push(val);
        }
        Ok(row_data)
    }).map_err(|e| format!("Failed to query parent rows: {}", e))?;
    
    for row_result in rows {
        if let Ok(row_data) = row_result {
            parent_rows.push(row_data);
        }
    }
    
    info!("Found {} parent rows to copy", parent_rows.len());
    
    // Start transaction
    let tx = conn.unchecked_transaction()
        .map_err(|e| format!("Failed to start transaction: {}", e))?;
    
    let mut child_row_index = 0i64;
    
    for parent_row in parent_rows {
        // Get parent_key value from parent row (key column + 2 for id, row_index)
        // Convert Value to String for comparison
        let parent_key = match parent_row.get(key_column_index + 2) {
            Some(rusqlite::types::Value::Text(s)) => s.clone(),
            Some(rusqlite::types::Value::Integer(i)) => i.to_string(),
            Some(rusqlite::types::Value::Real(r)) => r.to_string(),
            _ => String::new(),
        };
        
        if parent_key.is_empty() {
            continue; // Skip rows without a key
        }
        
        // Build normalized parent column map for efficient lookup
        let parent_col_map: std::collections::HashMap<String, usize> = parent_columns
            .iter()
            .enumerate()
            .map(|(idx, name)| (normalize_column_name(name), idx))
            .collect();
        
        if child_row_index == 0 {
            info!("======================================");
            info!("FIRST ROW PROCESSING - Column mapping:");
            info!("  Structure expects {} columns to copy", columns_to_copy.len());
            for (i, col) in columns_to_copy.iter().enumerate() {
                let normalized = normalize_column_name(&col.header);
                let found = parent_col_map.contains_key(&normalized);
                info!("    [{}] '{}' (normalized: '{}') -> found in parent: {}", 
                    i, col.header, normalized, found);
            }
            info!("======================================");
        }
        
        // Only log every 1000th row to reduce spam
        if child_row_index % 1000 == 0 {
            info!(
                "Processing parent row {}: parent_key='{}', parent has {} columns",
                child_row_index,
                parent_key,
                parent_columns.len()
            );
        }
        
        // Collect columns in order from structure_columns: grand_N_parent, parent_key, then data columns
        let mut insert_columns: Vec<String> = Vec::new();
        let mut insert_values: Vec<rusqlite::types::Value> = Vec::new();
        
        // Process all columns from structure_columns (excluding row_index which is already filtered)
        for col in columns_to_copy.iter() {
            let col_header = &col.header;
            
            // Handle grand_N_parent columns
            if col_header.starts_with("grand_") && col_header.ends_with("_parent") {
                let n_str = col_header.trim_start_matches("grand_").trim_end_matches("_parent");
                if let Ok(n) = n_str.parse::<usize>() {
                    let value = if n == 1 {
                        // grand_1_parent = parent's parent_key
                        let parent_key_normalized = normalize_column_name("parent_key");
                        if let Some(&parent_key_idx) = parent_col_map.get(&parent_key_normalized) {
                            let val = parent_row.get(parent_key_idx).cloned().unwrap_or(rusqlite::types::Value::Null);
                            // Only log every 1000th row or first row
                            if child_row_index % 1000 == 0 || child_row_index == 0 {
                                info!("  {} <- parent's parent_key (col {}) = {:?}", col_header, parent_key_idx, val);
                            }
                            val
                        } else {
                            // Only log once at first row
                            if child_row_index == 0 {
                                info!("  {}: parent has no parent_key (root table), using NULL", col_header);
                            }
                            rusqlite::types::Value::Null
                        }
                    } else {
                        // grand_N_parent = parent's grand_(N-1)_parent
                        let source_name = format!("grand_{}_parent", n - 1);
                        let source_normalized = normalize_column_name(&source_name);
                        if let Some(&source_idx) = parent_col_map.get(&source_normalized) {
                            let val = parent_row.get(source_idx).cloned().unwrap_or(rusqlite::types::Value::Null);
                            // Only log every 1000th row or first row
                            if child_row_index % 1000 == 0 || child_row_index == 0 {
                                info!("  {} <- parent's {} (col {}) = {:?}", col_header, source_name, source_idx, val);
                            }
                            val
                        } else {
                            // Only log once at first row
                            if child_row_index == 0 {
                                info!("  {}: parent has no {}, using NULL", col_header, source_name);
                            }
                            rusqlite::types::Value::Null
                        }
                    };
                    insert_columns.push(col_header.clone());
                    insert_values.push(value);
                }
                continue;
            }
            
            // Handle parent_key column
            if col_header.eq_ignore_ascii_case("parent_key") {
                insert_columns.push(col_header.clone());
                insert_values.push(rusqlite::types::Value::Text(parent_key.clone()));
                continue;
            }
            
            // Regular data column - find by normalized name in parent table
            let col_normalized = normalize_column_name(col_header);
            if let Some(&parent_col_idx) = parent_col_map.get(&col_normalized) {
                let mut value = parent_row.get(parent_col_idx).cloned().unwrap_or(rusqlite::types::Value::Null);
                
                // Normalize Bool values to TEXT "true"/"false" for Bool columns
                // This ensures the Bool validator works correctly
                if col.data_type == crate::sheets::definitions::ColumnDataType::Bool {
                    value = match &value {
                        rusqlite::types::Value::Text(s) => {
                            // Normalize text boolean representations to "true"/"false"
                            if s.eq_ignore_ascii_case("true") || s == "1" {
                                rusqlite::types::Value::Text("true".to_string())
                            } else {
                                rusqlite::types::Value::Text("false".to_string())
                            }
                        },
                        rusqlite::types::Value::Integer(i) => {
                            // Convert INTEGER 1/0 to TEXT "true"/"false"
                            if *i != 0 {
                                rusqlite::types::Value::Text("true".to_string())
                            } else {
                                rusqlite::types::Value::Text("false".to_string())
                            }
                        },
                        rusqlite::types::Value::Null => rusqlite::types::Value::Text("false".to_string()),
                        _ => rusqlite::types::Value::Text("false".to_string()),
                    };
                }
                
                // Only log every 1000th row or first row
                if child_row_index % 1000 == 0 || child_row_index == 0 {
                    info!("  {} <- parent's {} (col {}) = {:?}", col_header, parent_columns[parent_col_idx], parent_col_idx, value);
                }
                insert_columns.push(col_header.clone());
                insert_values.push(value);
            } else {
                warn!("  {} NOT FOUND in parent table, using NULL", col_header);
                insert_columns.push(col_header.clone());
                insert_values.push(rusqlite::types::Value::Null);
            }
        }
        
        if insert_columns.is_empty() {
            warn!("No columns to insert for parent_key='{}

', skipping", parent_key);
           continue;
        }
        
        // Build INSERT statement: row_index first, then all insert_columns in order
        let quoted_columns: Vec<String> = insert_columns.iter().map(|c| format!("\"{}\"", c)).collect();
        let columns_str = quoted_columns.join(", ");
        let placeholders = std::iter::repeat("?").take(1 + insert_columns.len()).collect::<Vec<_>>().join(", ");
        let insert_sql = format!(
            "INSERT INTO \"{}\" (row_index, {}) VALUES ({})",
            structure_table_name, columns_str, placeholders
        );
        
        // Only log every 1000th row to reduce spam
        if child_row_index % 1000 == 0 {
            info!(
                "Inserting child row {}: columns=[{}]",
                child_row_index,
                insert_columns.join(", ")
            );
        }
        
        // Prepare parameters: row_index first, then all insert_values (already as Value types)
        let mut params: Vec<rusqlite::types::Value> = Vec::with_capacity(1 + insert_values.len());
        params.push(rusqlite::types::Value::Integer(child_row_index));
        params.extend(insert_values); // insert_values is already Vec<Value>
        
        // Execute insert
        tx.execute(&insert_sql, rusqlite::params_from_iter(params.iter()))
            .map_err(|e| format!("Failed to insert child row: {}", e))?;
        
        child_row_index += 1;
    }
    
    // Commit transaction
    tx.commit().map_err(|e| format!("Failed to commit transaction: {}", e))?;
    
    info!("Successfully copied {} rows to structure table", child_row_index);
    Ok(())
}
