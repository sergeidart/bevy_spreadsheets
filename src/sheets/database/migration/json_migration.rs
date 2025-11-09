// src/sheets/database/migration/json_migration.rs

use bevy::prelude::*;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
// Use modern Base64 engine API (deprecated base64::encode replaced)
use base64::Engine as _;

use super::super::error::{DbError, DbResult};
use super::super::{schema, writer::DbWriter, daemon_client::{DaemonClient, Statement}};
use super::io_helpers::IoHelpers;
use super::json_extractor::{expand_value_to_rows, parse_cell_json};
use crate::sheets::definitions::{ColumnValidator, SheetMetadata};

#[derive(Debug, Clone, Default)]
pub struct MigrationReport {
    pub sheets_migrated: usize,
    pub sheets_failed: usize,
    pub failed_sheets: Vec<(String, String)>, // (sheet_name, error_message)
    pub linked_sheets_found: Vec<String>,
}

pub struct JsonMigration;

impl JsonMigration {
    /// Migrate a single sheet from JSON files to database
    pub fn migrate_sheet_from_json(
        conn: &mut Connection,
        json_data_path: &Path,
        json_meta_path: &Path,
        table_name: &str,
        display_order: Option<i32>,
        mut on_rows_chunk: Option<&mut dyn FnMut(usize)>,
        daemon_client: &DaemonClient,
    ) -> DbResult<()> {
        info!("Migrating sheet '{}' from JSON files...", table_name);

        // 1. Load JSON metadata and grid
        let metadata = IoHelpers::load_metadata(json_meta_path)?;
        let grid = IoHelpers::load_grid_data(json_data_path)?;

        // 2. Create schema
        let tx = conn.transaction()?;

        schema::ensure_global_metadata_table(&tx, daemon_client)?;
        schema::create_data_table(table_name, &metadata.columns, daemon_client)?;
        schema::create_metadata_table(table_name, &metadata, daemon_client)?;
        schema::create_ai_groups_table(&tx, table_name, &metadata, daemon_client)?;
        schema::insert_table_metadata(table_name, &metadata, display_order, daemon_client)?;

        // 3. Handle structure columns: create structure tables and their metadata
        let mut structure_fields_by_col: HashMap<
            usize,
            Vec<crate::sheets::definitions::StructureFieldDefinition>,
        > = HashMap::new();
        
        for (col_idx, col) in metadata.columns.iter().enumerate() {
            if matches!(col.validator, Some(ColumnValidator::Structure)) {
                if let Some(schema_fields) = &col.structure_schema {
                    schema::create_structure_table(&tx, table_name, col, None, daemon_client)?;

                    let structure_table = format!("{}_{}", table_name, col.header);

                    // Create metadata table for the structure sheet (columns only)
                    let structure_meta_name = format!("{}_Metadata", structure_table);
                    let create_meta_stmt = Statement {
                        sql: format!(
                            "CREATE TABLE IF NOT EXISTS \"{}\" (
                                id INTEGER PRIMARY KEY AUTOINCREMENT,
                                column_index INTEGER UNIQUE NOT NULL,
                                column_name TEXT NOT NULL UNIQUE,
                                data_type TEXT NOT NULL,
                                validator_type TEXT,
                                validator_config TEXT,
                                ai_context TEXT,
                                filter_expr TEXT,
                                ai_enable_row_generation INTEGER DEFAULT 0,
                                ai_include_in_send INTEGER DEFAULT 1,
                                deleted INTEGER DEFAULT 0
                            )",
                            structure_meta_name
                        ),
                        params: vec![],
                    };
                    daemon_client.exec_batch(vec![create_meta_stmt])
                        .map_err(|e| DbError::MigrationFailed(format!("Failed to create structure metadata table: {}", e)))?;

                    // Insert structure fields metadata (index starts at 0 for first structure field)
                    let mut insert_stmts = Vec::new();
                    for (sidx, field) in schema_fields.iter().enumerate() {
                        let ai_ctx: Option<String> = field.ai_context.clone();
                        let filter_expr: Option<String> = field.filter.clone();
                        let include_in_send: i32 = field.ai_include_in_send.unwrap_or(true) as i32;
                        let allow_add: i32 = field.ai_enable_row_generation.unwrap_or(false) as i32;
                        insert_stmts.push(Statement {
                            sql: format!(
                                "INSERT OR REPLACE INTO \"{}\" 
                                 (column_index, column_name, data_type, validator_type, validator_config, ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send, deleted)
                                 VALUES (?, ?, ?, NULL, NULL, ?, ?, ?, ?, ?)",
                                structure_meta_name
                            ),
                            params: vec![
                                serde_json::Value::Number((sidx as i32).into()),
                                serde_json::Value::String(field.header.clone()),
                                serde_json::Value::String(format!("{:?}", field.data_type)),
                                ai_ctx.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                                filter_expr.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                                serde_json::Value::Number(allow_add.into()),
                                serde_json::Value::Number(include_in_send.into()),
                                serde_json::Value::Number(0.into()), // deleted flag
                            ],
                        });
                    }
                    if !insert_stmts.is_empty() {
                        daemon_client.exec_batch(insert_stmts)
                            .map_err(|e| DbError::MigrationFailed(format!("Failed to insert structure field metadata: {}", e)))?;
                    }

                    structure_fields_by_col.insert(col_idx, schema_fields.clone());
                }
            }
        }

        // 4. Insert data for main table (with per-1k row chunk callback)
        let mut maybe_cb = on_rows_chunk.as_mut();
        DbWriter::insert_grid_data_with_progress(&tx, table_name, &grid, &metadata, |rows_done| {
            if let Some(cb) = maybe_cb.as_deref_mut() {
                cb(rows_done);
            }
        }, daemon_client)?;
        // Always emit a final main-progress tick so small sheets (<1000 rows) still report progress
        if let Some(cb) = maybe_cb.as_deref_mut() {
            cb(grid.len());
        }

        // 5. Extract inline JSON from structure columns and populate structure tables
        if !structure_fields_by_col.is_empty() {
            Self::migrate_structure_data(
                &tx,
                table_name,
                &grid,
                &metadata,
                &structure_fields_by_col,
                maybe_cb,
                daemon_client,
            )?;
        }

        tx.commit()?;

        info!("Successfully migrated sheet '{}'", table_name);
        Ok(())
    }

    /// Extract and migrate structure column data
    fn migrate_structure_data(
        tx: &rusqlite::Transaction,
        table_name: &str,
        grid: &[Vec<String>],
        metadata: &SheetMetadata,
        structure_fields_by_col: &HashMap<usize, Vec<crate::sheets::definitions::StructureFieldDefinition>>,
        mut maybe_cb: Option<&mut &mut dyn FnMut(usize)>,
        daemon_client: &DaemonClient,
    ) -> DbResult<()> {
        // Track aggregate count of inserted structure rows to emit per-1k updates
        let main_total_rows = grid.len();
        let mut struct_total_inserted: usize = 0;
        
        // Build a map of main row id by row_index so we can set parent_id
        let mut id_stmt =
            tx.prepare(&format!("SELECT id, row_index FROM \"{}\"", table_name))?;
        let mut id_map: HashMap<i32, i64> = HashMap::new();
        let rows = id_stmt.query_map([], |r| Ok((r.get::<_, i32>(1)?, r.get::<_, i64>(0)?)))?;
        for row in rows {
            let (row_index, id_val) = row?;
            id_map.insert(row_index, id_val);
        }
        
        // Pre-calculate starting row_index for each structure table to avoid race conditions
        // Each structure table maintains its own global sequential row_index counter
        let mut row_index_counters: HashMap<String, i32> = HashMap::new();
        for (&col_idx, _schema_fields) in structure_fields_by_col {
            let structure_table = format!("{}_{}", table_name, metadata.columns[col_idx].header);
            let max_row_index: Option<i32> = tx.query_row(
                &format!("SELECT MAX(row_index) FROM \"{}\"", structure_table),
                [],
                |r| r.get(0),
            ).unwrap_or(None);
            let start_index = max_row_index.unwrap_or(-1) + 1;
            row_index_counters.insert(structure_table.clone(), start_index);
            info!("Structure table '{}': starting row_index at {}", structure_table, start_index);
        }

        for (row_index, row) in grid.iter().enumerate() {
            let parent_id = match id_map.get(&(row_index as i32)) {
                Some(v) => *v,
                None => continue,
            };
            
            for (&col_idx, schema_fields) in structure_fields_by_col {
                if let Some(cell) = row.get(col_idx) {
                    if cell.trim().is_empty() {
                        continue;
                    }
                }
                
                let cell_json = row.get(col_idx).cloned().unwrap_or_default();
                if cell_json.trim().is_empty() {
                    continue;
                }
                
                let structure_table =
                    format!("{}_{}", table_name, metadata.columns[col_idx].header);

                let parsed = parse_cell_json(&cell_json);
                let rows_to_insert: Vec<Vec<String>> = expand_value_to_rows(
                    parsed,
                    schema_fields,
                    &metadata.columns[col_idx].header,
                );

                if rows_to_insert.is_empty() {
                    continue;
                }

                // Insert rows: columns are (parent_id, row_index, parent_key, <fields...>)
                let field_cols = schema_fields
                    .iter()
                    .map(|f| format!("\"{}\"", f.header))
                    .collect::<Vec<_>>()
                    .join(", ");
                let placeholders = std::iter::repeat("?")
                    .take(3 + schema_fields.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let insert_sql = format!(
                    "INSERT INTO \"{}\" (parent_id, row_index, parent_key, {}) VALUES ({})",
                    structure_table, field_cols, placeholders
                );
                // Prepare removed: we batch through daemon, no direct stmt.execute
                
                for (_sidx, srow) in rows_to_insert.iter().enumerate() {
                    // Ensure the row width matches the schema width
                    let mut row_padded: Vec<String> = srow.clone();
                    let cols = schema_fields.len();
                    if row_padded.len() < cols {
                        row_padded.resize(cols, String::new());
                    }
                    if row_padded.len() > cols {
                        row_padded.truncate(cols);
                    }
                    
                    // Get and increment the global row_index counter for this structure table
                    let current_row_index = row_index_counters.get_mut(&structure_table)
                        .expect("row_index counter should exist");
                    let row_idx_value = *current_row_index;
                    *current_row_index += 1;
                    
                    let mut params: Vec<rusqlite::types::Value> =
                        Vec::with_capacity(3 + schema_fields.len());
                    params.push(rusqlite::types::Value::Integer(parent_id));
                    // Use global sequential row_index from counter
                    params.push(rusqlite::types::Value::Integer(row_idx_value as i64));
                    
                    // Parent key: use value from key parent column if set, otherwise fallback to a sensible main-table key
                    let parent_key = metadata
                        .columns
                        .get(col_idx)
                        .and_then(|c| c.structure_key_parent_column_index)
                        .and_then(|kidx| row.get(kidx).cloned())
                        .or_else(|| {
                            // Fallback order: "Key" -> "Name" -> "ID" (case-insensitive)
                            let mut fallback_idx: Option<usize> = None;
                            for (i, cdef) in metadata.columns.iter().enumerate() {
                                if cdef.header.eq_ignore_ascii_case("Key") {
                                    fallback_idx = Some(i);
                                    break;
                                }
                            }
                            if fallback_idx.is_none() {
                                for (i, cdef) in metadata.columns.iter().enumerate() {
                                    if cdef.header.eq_ignore_ascii_case("Name") {
                                        fallback_idx = Some(i);
                                        break;
                                    }
                                }
                            }
                            if fallback_idx.is_none() {
                                for (i, cdef) in metadata.columns.iter().enumerate() {
                                    if cdef.header.eq_ignore_ascii_case("ID") {
                                        fallback_idx = Some(i);
                                        break;
                                    }
                                }
                            }
                            fallback_idx.and_then(|i| row.get(i).cloned())
                        })
                        .unwrap_or_default();
                    params.push(rusqlite::types::Value::Text(parent_key));
                    
                    for cell in row_padded {
                        params.push(rusqlite::types::Value::Text(cell));
                    }
                    
                    // Redirect structure row insertion through daemon for write serialization
                    {
                        use crate::sheets::database::daemon_client::Statement as DStatement;
                        let mut json_params: Vec<serde_json::Value> = Vec::with_capacity(params.len());
                        for p in params.iter() {
                            use rusqlite::types::Value as Rv;
                            match p {
                                Rv::Null => json_params.push(serde_json::Value::Null),
                                Rv::Integer(i) => json_params.push(serde_json::Value::Number((*i).into())),
                                Rv::Real(f) => json_params.push(serde_json::json!(*f)),
                                Rv::Text(s) => json_params.push(serde_json::Value::String(s.clone())),
                                Rv::Blob(b) => {
                                    // Encode blob using explicit engine to avoid deprecated base64::encode warning
                                    let encoded = base64::engine::general_purpose::STANDARD.encode(b);
                                    json_params.push(serde_json::Value::String(encoded));
                                },
                            }
                        }
                        let insert_stmt = DStatement { sql: insert_sql.clone(), params: json_params };
                        daemon_client.exec_batch(vec![insert_stmt])
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
                    }
                    struct_total_inserted += 1;
                    
                    if struct_total_inserted > 0 && struct_total_inserted % 1000 == 0 {
                        if let Some(cb) = maybe_cb.as_deref_mut() {
                            // Report combined progress: main rows + aggregated structure rows
                            cb(main_total_rows + struct_total_inserted);
                        }
                    }
                }
            }
        }
        
        // Emit a final structures tick if some were inserted but didn't reach a 1000 multiple
        if struct_total_inserted > 0 {
            if let Some(cb) = maybe_cb.as_deref_mut() {
                cb(main_total_rows + struct_total_inserted);
            }
        }

        Ok(())
    }
}
