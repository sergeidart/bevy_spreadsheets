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
/// Creates one initial child row per parent row, copying specified columns from parent to child
/// Skips parents that already have child rows (preserves existing data)
pub fn copy_parent_content_to_structure_table(
    conn: &rusqlite::Connection,
    parent_table_name: &str,
    structure_table_name: &str,
    parent_col_def: &ColumnDefinition,
    _structure_columns: &[ColumnDefinition],
    db_path: &std::path::Path,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) -> Result<(), String> {
    use crate::sheets::database::daemon_client::Statement;
    
    // Get the structure schema - tells us which columns to copy from parent
    let schema_fields = parent_col_def.structure_schema.as_ref()
        .ok_or_else(|| "Structure column missing schema".to_string())?;
    
    // Find the key column index in parent metadata
    let key_column_index_in_metadata = parent_col_def
        .structure_key_parent_column_index
        .ok_or_else(|| "Structure column missing key_parent_column_index".to_string())?;
    
    info!("======================================");
    info!("COPY STRUCTURE DATA: {} -> {}", parent_table_name, structure_table_name);
    info!("Structure column: '{}', key column index in metadata: {}", parent_col_def.header, key_column_index_in_metadata);
    info!("Will create one child row per parent row, copying {} columns:", schema_fields.len());
    for (i, field) in schema_fields.iter().enumerate() {
        info!("  [{}] '{}'", i, field.header);
    }
    
    // Get all column names from parent table
    let parent_columns: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info(\"{}\")", parent_table_name))
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(1))
                .and_then(|mapped_rows| mapped_rows.collect::<Result<Vec<_>, _>>())
        })
        .map_err(|e| format!("Failed to query parent table schema: {}", e))?;
    
    info!("Parent table DB columns: {:?}", parent_columns);
    
    // Find the row_index column in the parent table (needed for parent_key)
    let parent_row_index_col = parent_columns
        .iter()
        .position(|col_name| col_name.eq_ignore_ascii_case("row_index"))
        .ok_or_else(|| "Parent table missing row_index column".to_string())?;
    
    info!("Parent row_index column at DB index: {}", parent_row_index_col);
    
    // Build a map of normalized parent column names to their indices
    let parent_col_map: std::collections::HashMap<String, usize> = parent_columns
        .iter()
        .enumerate()
        .map(|(idx, name)| (normalize_column_name(name), idx))
        .collect();
    
    info!("======================================");
    
    // Get existing parent_keys that already have child rows
    let existing_parents_query = format!(
        "SELECT DISTINCT parent_key FROM \"{}\"",
        structure_table_name
    );
    let mut existing_parents_set = std::collections::HashSet::new();
    if let Ok(mut stmt) = conn.prepare(&existing_parents_query) {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for row_result in rows {
                if let Ok(parent_key) = row_result {
                    existing_parents_set.insert(parent_key);
                }
            }
        }
    }
    
    if !existing_parents_set.is_empty() {
        info!("Found {} parents that already have child rows - will skip them", existing_parents_set.len());
    }
    
    // Read all rows from parent table
    let query = format!("SELECT * FROM \"{}\" ORDER BY row_index", parent_table_name);
    let mut stmt = conn.prepare(&query)
        .map_err(|e| format!("Failed to prepare parent query: {}", e))?;
    
    let column_count = parent_columns.len();
    let mut parent_rows: Vec<Vec<rusqlite::types::Value>> = Vec::new();
    
    let rows = stmt.query_map([], |row| {
        let mut row_data = Vec::with_capacity(column_count);
        for i in 0..column_count {
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
    
    info!("Found {} parent rows to process", parent_rows.len());
    
    let mut child_row_index = 0i64;
    let mut insert_statements = Vec::new();
    let mut skipped_count = 0usize;
    let mut first_row_logged = false;
    
    for parent_row in parent_rows {
        // CRITICAL: parent_key is the parent's row_index value (not content from key column)
        // This links the child row to its parent row
        let parent_key = match parent_row.get(parent_row_index_col) {
            Some(rusqlite::types::Value::Integer(i)) => i.to_string(),
            Some(rusqlite::types::Value::Text(s)) => s.clone(),
            Some(rusqlite::types::Value::Real(r)) => r.to_string(),
            _ => {
                warn!("Parent row has invalid row_index, skipping");
                continue;
            }
        };
        
        if parent_key.is_empty() {
            continue; // Skip rows without a row_index
        }
        
        // Log first row for debugging
        if !first_row_logged {
            info!("  First parent row: parent_key (row_index) = {}", parent_key);
            if let Some(val) = parent_row.get(parent_row_index_col) {
                info!("    Raw row_index value: {:?}", val);
            }
            first_row_logged = true;
        }
        
        // Skip if this parent already has child rows
        if existing_parents_set.contains(&parent_key) {
            skipped_count += 1;
            continue;
        }
        
        // Build column list and values for INSERT
        let mut columns = vec!["row_index".to_string(), "parent_key".to_string()];
        let mut values: Vec<serde_json::Value> = vec![
            serde_json::Value::Number(child_row_index.into()),
            serde_json::Value::String(parent_key.clone()),
        ];
        
        // Copy data columns from parent based on schema_fields
        for field in schema_fields.iter() {
            columns.push(field.header.clone());
            
            // Find this column in parent by normalized name
            let field_normalized = normalize_column_name(&field.header);
            if let Some(&parent_col_idx) = parent_col_map.get(&field_normalized) {
                // Copy value from parent row
                let value = parent_row.get(parent_col_idx);
                let json_value = match value {
                    Some(rusqlite::types::Value::Text(s)) => serde_json::Value::String(s.clone()),
                    Some(rusqlite::types::Value::Integer(i)) => serde_json::Value::Number((*i).into()),
                    Some(rusqlite::types::Value::Real(r)) => {
                        serde_json::Number::from_f64(*r)
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::Null)
                    },
                    Some(rusqlite::types::Value::Blob(b)) => {
                        serde_json::Value::String(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b))
                    },
                    _ => serde_json::Value::Null,
                };
                
                if child_row_index == 0 {
                    info!("  Copying '{}' from parent[{}] = {:?}", field.header, parent_col_idx, value);
                }
                
                values.push(json_value);
            } else {
                // Column not found in parent, use NULL
                if child_row_index == 0 {
                    warn!("  Column '{}' not found in parent table, using NULL", field.header);
                }
                values.push(serde_json::Value::Null);
            }
        }
        
        // Build INSERT SQL
        let quoted_columns: Vec<String> = columns.iter().map(|c| format!("\"{}\"", c)).collect();
        let columns_str = quoted_columns.join(", ");
        let placeholders = std::iter::repeat("?").take(columns.len()).collect::<Vec<_>>().join(", ");
        let insert_sql = format!(
            "INSERT INTO \"{}\" ({}) VALUES ({})",
            structure_table_name, columns_str, placeholders
        );
        
        insert_statements.push(Statement {
            sql: insert_sql,
            params: values,
        });
        
        child_row_index += 1;
    }
    
    // Execute all inserts through daemon
    if !insert_statements.is_empty() {
        info!("Inserting {} child rows into structure table", child_row_index);
        daemon_client.exec_batch(insert_statements, db_path.file_name().and_then(|n| n.to_str()))
            .map_err(|e| format!("Failed to insert child rows: {}", e))?;
    }
    
    if skipped_count > 0 {
        info!("Skipped {} parents that already have child rows", skipped_count);
    }
    info!("Successfully created {} new child rows (1 per parent row)", child_row_index);
    info!("======================================");
    Ok(())
}
