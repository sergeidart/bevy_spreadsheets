// src/sheets/database/migration.rs

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use bevy::prelude::*;

use super::error::{DbResult, DbError};
use super::{schema, writer::DbWriter};
use crate::sheets::definitions::{SheetMetadata, SheetGridData, ColumnValidator};

#[derive(Debug, Clone, Default)]
pub struct MigrationReport {
    pub sheets_migrated: usize,
    pub sheets_failed: usize,
    pub failed_sheets: Vec<(String, String)>, // (sheet_name, error_message)
    pub linked_sheets_found: Vec<String>,
}

pub struct MigrationTools;

impl MigrationTools {
    /// Migrate a single sheet from JSON files to database
    pub fn migrate_sheet_from_json(
        conn: &mut Connection,
        json_data_path: &Path,
        json_meta_path: &Path,
        table_name: &str,
        display_order: Option<i32>,
        mut on_rows_chunk: Option<&mut dyn FnMut(usize)>,
    ) -> DbResult<()> {
        info!("Migrating sheet '{}' from JSON files...", table_name);
        
        // 1. Load JSON metadata
        let meta_content = std::fs::read_to_string(json_meta_path)?;
        let metadata: SheetMetadata = serde_json::from_str(&meta_content)?;
        
        // 2. Load JSON grid
        let data_content = std::fs::read_to_string(json_data_path)?;
        let grid: Vec<Vec<String>> = serde_json::from_str(&data_content)?;
        
        // 3. Create schema
        let tx = conn.transaction()?;
        
        schema::ensure_global_metadata_table(&tx)?;
        schema::create_data_table(&tx, table_name, &metadata.columns)?;
        schema::create_metadata_table(&tx, table_name, &metadata)?;
        schema::create_ai_groups_table(&tx, table_name, &metadata)?;
        schema::insert_table_metadata(&tx, table_name, &metadata, display_order)?;
        
        // 4. Handle structure columns: create structure tables and their metadata
        // Also prepare parsers to extract inline JSON into structure rows
        let mut structure_fields_by_col: HashMap<usize, Vec<crate::sheets::definitions::StructureFieldDefinition>> = HashMap::new();
        for (col_idx, col) in metadata.columns.iter().enumerate() {
            if matches!(col.validator, Some(ColumnValidator::Structure)) {
                if let Some(schema_fields) = &col.structure_schema {
                    schema::create_structure_table(&tx, table_name, col)?;

                    let structure_table = format!("{}_{}", table_name, col.header);

                    // Create metadata table for the structure sheet (columns only)
                    let structure_meta_name = format!("{}_Metadata", structure_table);
                    tx.execute(
                        &format!(
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
                                ai_include_in_send INTEGER DEFAULT 1
                            )",
                            structure_meta_name
                        ),
                        [],
                    )?;

                    // Insert structure fields metadata (index starts at 0 for first structure field)
                    for (sidx, field) in schema_fields.iter().enumerate() {
                        let ai_ctx: Option<String> = field.ai_context.clone();
                        let filter_expr: Option<String> = field.filter.clone();
                        let include_in_send: i32 = field.ai_include_in_send.unwrap_or(true) as i32;
                        let allow_add: i32 = field.ai_enable_row_generation.unwrap_or(false) as i32;
                        tx.execute(
                            &format!(
                                "INSERT OR REPLACE INTO \"{}\" 
                                 (column_index, column_name, data_type, validator_type, validator_config, ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send)
                                 VALUES (?, ?, ?, NULL, NULL, ?, ?, ?, ?)",
                                structure_meta_name
                            ),
                            rusqlite::params![
                                sidx as i32,
                                &field.header,
                                format!("{:?}", field.data_type),
                                ai_ctx,
                                filter_expr,
                                allow_add,
                                include_in_send,
                            ],
                        )?;
                    }

                    structure_fields_by_col.insert(col_idx, schema_fields.clone());
                }
            }
        }
        
    // 5. Insert data for main table (with per-1k row chunk callback)
        let mut maybe_cb = on_rows_chunk.as_mut();
        DbWriter::insert_grid_data_with_progress(&tx, table_name, &grid, &metadata, |rows_done| {
            if let Some(cb) = maybe_cb.as_deref_mut() { cb(rows_done); }
        })?;
        // Always emit a final main-progress tick so small sheets (<1000 rows) still report progress
        if let Some(cb) = maybe_cb.as_deref_mut() {
            cb(grid.len());
        }

        // 6. Extract inline JSON from structure columns and populate structure tables
        if !structure_fields_by_col.is_empty() {
            // Track aggregate count of inserted structure rows to emit per-1k updates
            let main_total_rows = grid.len();
            let mut struct_total_inserted: usize = 0;
            // Build a map of main row id by row_index so we can set parent_id
            let mut id_stmt = tx.prepare(&format!("SELECT id, row_index FROM \"{}\"", table_name))?;
            let mut id_map: HashMap<i32, i64> = HashMap::new();
            let rows = id_stmt.query_map([], |r| Ok((r.get::<_, i32>(1)?, r.get::<_, i64>(0)?)))?;
            for row in rows { let (row_index, id_val) = row?; id_map.insert(row_index, id_val); }

            for (row_index, row) in grid.iter().enumerate() {
                let parent_id = match id_map.get(&(row_index as i32)) { Some(v) => *v, None => continue };
                for (&col_idx, schema_fields) in &structure_fields_by_col {
                    if let Some(cell) = row.get(col_idx) { if cell.trim().is_empty() { continue; } }
                    let cell_json = row.get(col_idx).cloned().unwrap_or_default();
                    if cell_json.trim().is_empty() { continue; }
                    let structure_table = format!("{}_{}", table_name, metadata.columns[col_idx].header);

                    // Helper: convert any JSON value to string losslessly enough for grid
                    fn json_value_to_string(v: &serde_json::Value) -> String {
                        match v {
                            serde_json::Value::Null => String::new(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            // For nested structures, serialize compactly
                            _ => v.to_string(),
                        }
                    }

                    fn normalize_key_str(s: &str) -> String {
                        // Lowercase and strip non-alphanumeric to better match headers with minor differences
                        s.chars()
                            .filter(|c| c.is_alphanumeric())
                            .map(|c| c.to_ascii_lowercase())
                            .collect::<String>()
                    }

                    fn row_has_any_value(row: &[String]) -> bool {
                        row.iter().any(|s| !s.trim().is_empty())
                    }

                    // Parse JSON value, support double-encoded JSON strings
                    fn parse_cell_json(cell: &str) -> serde_json::Value {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(cell) {
                            // If it's a string that looks like JSON, try parsing once more
                            if let serde_json::Value::String(s) = &v {
                                let st = s.trim();
                                if (st.starts_with('[') && st.ends_with(']')) || (st.starts_with('{') && st.ends_with('}')) {
                                    if let Ok(v2) = serde_json::from_str::<serde_json::Value>(st) { return v2; }
                                }
                            }
                            v
                        } else {
                            // Try wrap single object as array
                            let ts = cell.trim();
                            if ts.starts_with('{') && ts.ends_with('}') {
                                let wrapped = format!("[{}]", ts);
                                serde_json::from_str(&wrapped).unwrap_or(serde_json::Value::Null)
                            } else {
                                serde_json::Value::Null
                            }
                        }
                    }

                    // Expand Value into rows honoring schema ordering; handle common wrappers
                    fn expand_value_to_rows(val: serde_json::Value, schema_fields: &[crate::sheets::definitions::StructureFieldDefinition], structure_header: &str) -> Vec<Vec<String>> {
                        let header_norm = normalize_key_str(structure_header);
                        match val {
                            serde_json::Value::Array(arr) => {
                                if arr.is_empty() { return Vec::new(); }
                                if arr.iter().all(|v| v.is_array()) {
                                    // [[...], [...]]: map by position
                                    let mut out: Vec<Vec<String>> = Vec::with_capacity(arr.len());
                                    for inner in arr.into_iter() {
                                        if let Some(ia) = inner.as_array() {
                                            let cols = schema_fields.len();
                                            let mut row_vec: Vec<String> = ia.iter().take(cols).map(json_value_to_string).collect();
                                            if row_vec.len() < cols { row_vec.resize(cols, String::new()); }
                                            if row_has_any_value(&row_vec) { out.push(row_vec); }
                                        }
                                    }
                                    return out;
                                }
                                if arr.iter().all(|v| v.is_object()) {
                                    // [ { field: val }, ... ] map by schema order with normalized key matching
                                    let mut out: Vec<Vec<String>> = Vec::with_capacity(arr.len());
                                    for obj in arr.into_iter() {
                                        let map = obj.as_object().cloned().unwrap_or_default();
                                        let mut norm_map: std::collections::HashMap<String, &serde_json::Value> = std::collections::HashMap::new();
                                        for (k, v) in &map { norm_map.insert(normalize_key_str(k), v); }
                                        let mut row_vec = Vec::with_capacity(schema_fields.len());
                                        for f in schema_fields {
                                            let key_norm = normalize_key_str(&f.header);
                                            let val = map.get(&f.header)
                                                .or_else(|| norm_map.get(&key_norm).copied());
                                            row_vec.push(val.map(json_value_to_string).unwrap_or_default());
                                        }
                                        if row_has_any_value(&row_vec) { out.push(row_vec); }
                                    }
                                    return out;
                                }
                                // Array of primitives or mixed -> map by position when possible
                                if !schema_fields.is_empty() {
                                    // Convert all items to strings (objects/arrays become compact JSON)
                                    let values: Vec<String> = arr.iter().map(json_value_to_string).collect();
                                    let cols = schema_fields.len();

                                    if cols == 1 {
                                        // Single-field schema: N rows, one per value
                                        let mut out: Vec<Vec<String>> = Vec::with_capacity(values.len());
                                        for v in values { if !v.trim().is_empty() { out.push(vec![v]); } }
                                        return out;
                                    }

                                    // Multi-field schema
                                    if values.len() == cols {
                                        // Exact fit -> single row by position
                                        let mut row_vec = values.into_iter().take(cols).collect::<Vec<_>>();
                                        if row_vec.len() < cols { row_vec.resize(cols, String::new()); }
                                        if row_has_any_value(&row_vec) { return vec![row_vec]; }
                                        return Vec::new();
                                    }
                                    if values.len() % cols == 0 {
                                        // Chunk into groups of cols
                                        let mut out: Vec<Vec<String>> = Vec::with_capacity(values.len() / cols);
                                        for chunk in values.chunks(cols) {
                                            let mut row_vec = chunk.iter().cloned().collect::<Vec<_>>();
                                            if row_vec.len() < cols { row_vec.resize(cols, String::new()); }
                                            if row_has_any_value(&row_vec) { out.push(row_vec); }
                                        }
                                        return out;
                                    }
                                    // Fallback: map first N values into one row
                                    let mut row_vec = values.into_iter().take(cols).collect::<Vec<_>>();
                                    if row_vec.len() < cols { row_vec.resize(cols, String::new()); }
                                    if row_has_any_value(&row_vec) { return vec![row_vec]; }
                                    return Vec::new();
                                }
                                Vec::new()
                            }
                            serde_json::Value::Object(map) => {
                                // If object contains an array under a key matching the structure header, prefer that
                                let mut norm_map: std::collections::HashMap<String, &serde_json::Value> = std::collections::HashMap::new();
                                for (k, v) in &map { norm_map.insert(normalize_key_str(k), v); }

                                if let Some(arr_val) = norm_map.get(&header_norm).and_then(|v| if v.is_array() { Some((*v).clone()) } else { None }) {
                                    return expand_value_to_rows(arr_val, schema_fields, structure_header);
                                }

                                // Then look for common wrapper keys
                                let candidate_keys = ["Rows","rows","items","Items","data","Data"]; 
                                if let Some((_, arr_val)) = map.iter().find(|(k, v)| v.is_array() && candidate_keys.contains(&k.as_str()))
                                    .or_else(|| map.iter().find(|(_, v)| v.is_array()))
                                {
                                    return expand_value_to_rows(arr_val.clone(), schema_fields, structure_header);
                                }
                                // Otherwise, map this object as a single row with normalized key matching
                                if schema_fields.is_empty() { return Vec::new(); }
                                let mut norm_map: std::collections::HashMap<String, &serde_json::Value> = std::collections::HashMap::new();
                                for (k, v) in &map { norm_map.insert(normalize_key_str(k), v); }
                                let mut row_vec = Vec::with_capacity(schema_fields.len());
                                for f in schema_fields {
                                    let key_norm = normalize_key_str(&f.header);
                                    let val = map.get(&f.header)
                                        .or_else(|| norm_map.get(&key_norm).copied());
                                    row_vec.push(val.map(json_value_to_string).unwrap_or_default());
                                }
                                if row_has_any_value(&row_vec) { vec![row_vec] } else { Vec::new() }
                            }
                            serde_json::Value::String(s) => {
                                // Try parse inner JSON
                                let inner = parse_cell_json(&s);
                                expand_value_to_rows(inner, schema_fields, structure_header)
                            }
                            _ => Vec::new()
                        }
                    }

                    let parsed = parse_cell_json(&cell_json);
                let rows_to_insert: Vec<Vec<String>> = expand_value_to_rows(parsed, schema_fields, &metadata.columns[col_idx].header);

                    if rows_to_insert.is_empty() { continue; }

                    // Insert rows: columns are (parent_id, row_index, parent_key, <fields...>)
                    let field_cols = schema_fields.iter().map(|f| format!("\"{}\"", f.header)).collect::<Vec<_>>().join(", ");
                    let placeholders = std::iter::repeat("?").take(3 + schema_fields.len()).collect::<Vec<_>>().join(", ");
                    let insert_sql = format!("INSERT INTO \"{}\" (parent_id, row_index, parent_key, {}) VALUES ({})", structure_table, field_cols, placeholders);
                    let mut stmt = tx.prepare(&insert_sql)?;
                    for (sidx, srow) in rows_to_insert.iter().enumerate() {
                        // Ensure the row width matches the schema width
                        let mut row_padded: Vec<String> = srow.clone();
                        let cols = schema_fields.len();
                        if row_padded.len() < cols { row_padded.resize(cols, String::new()); }
                        if row_padded.len() > cols { row_padded.truncate(cols); }
                        let mut params: Vec<rusqlite::types::Value> = Vec::with_capacity(3 + schema_fields.len());
                        params.push(rusqlite::types::Value::Integer(parent_id));
                        params.push(rusqlite::types::Value::Integer(sidx as i64));
                        // Parent key: use value from key parent column if set, otherwise fallback to a sensible main-table key
                        let parent_key = metadata.columns.get(col_idx)
                            .and_then(|c| c.structure_key_parent_column_index)
                            .and_then(|kidx| row.get(kidx).cloned())
                            .or_else(|| {
                                // Fallback order: "Key" -> "Name" -> "ID" (case-insensitive)
                                let mut fallback_idx: Option<usize> = None;
                                for (i, cdef) in metadata.columns.iter().enumerate() {
                                    if cdef.header.eq_ignore_ascii_case("Key") { fallback_idx = Some(i); break; }
                                }
                                if fallback_idx.is_none() {
                                    for (i, cdef) in metadata.columns.iter().enumerate() {
                                        if cdef.header.eq_ignore_ascii_case("Name") { fallback_idx = Some(i); break; }
                                    }
                                }
                                if fallback_idx.is_none() {
                                    for (i, cdef) in metadata.columns.iter().enumerate() {
                                        if cdef.header.eq_ignore_ascii_case("ID") { fallback_idx = Some(i); break; }
                                    }
                                }
                                fallback_idx.and_then(|i| row.get(i).cloned())
                            })
                            .unwrap_or_default();
                        params.push(rusqlite::types::Value::Text(parent_key));
                        for cell in row_padded { params.push(rusqlite::types::Value::Text(cell)); }
                        stmt.execute(rusqlite::params_from_iter(params.iter()))?;
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
        }
        
        tx.commit()?;
        
        info!("Successfully migrated sheet '{}'", table_name);
        Ok(())
    }
    
    /// Find all linked sheets referenced in metadata
    pub fn find_linked_sheets(metadata: &SheetMetadata) -> Vec<String> {
        let mut linked = HashSet::new();
        
        for col in &metadata.columns {
            if let Some(ColumnValidator::Linked { target_sheet_name, .. }) = &col.validator {
                linked.insert(target_sheet_name.clone());
            }
        }
        
        linked.into_iter().collect()
    }
    
    /// Scan folder for JSON pairs and their dependencies
    pub fn scan_json_folder(folder_path: &Path) -> DbResult<HashMap<String, JsonSheetPair>> {
        let mut sheets = HashMap::new();
        
        if !folder_path.exists() || !folder_path.is_dir() {
            return Err(DbError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Folder not found"
            )));
        }
        
        // Find all .json files (not .meta.json)
        for entry in std::fs::read_dir(folder_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".meta.json") {
                        continue;
                    }
                    
                    if name.ends_with(".json") {
                        let sheet_name = name.trim_end_matches(".json").to_string();
                        let meta_path = path.with_file_name(format!("{}.meta.json", sheet_name));
                        
                        if meta_path.exists() {
                            // Read metadata to find dependencies
                            let meta_content = std::fs::read_to_string(&meta_path)?;
                            let metadata: SheetMetadata = serde_json::from_str(&meta_content)
                                .map_err(|e| DbError::InvalidMetadata(e.to_string()))?;
                            
                            let dependencies = Self::find_linked_sheets(&metadata);
                            
                            sheets.insert(sheet_name.clone(), JsonSheetPair {
                                name: sheet_name,
                                data_path: path,
                                meta_path,
                                dependencies,
                                category: metadata.category.clone(),
                            });
                        }
                    }
                }
            }
        }
        
        Ok(sheets)
    }
    
    /// Migrate multiple sheets with dependency resolution
    pub fn migrate_folder_to_db(
        db_path: &Path,
        folder_path: &Path,
        create_new_db: bool,
    ) -> DbResult<MigrationReport> {
        let mut report = MigrationReport::default();
        
        // Scan folder
        let sheets = Self::scan_json_folder(folder_path)?;
        
        if sheets.is_empty() {
            return Err(DbError::MigrationFailed("No valid JSON sheet pairs found".into()));
        }
        
        info!("Found {} sheets to migrate", sheets.len());
        
        // Open or create database
        let mut conn = if create_new_db || !db_path.exists() {
            super::connection::DbConnection::create_new(db_path)?
        } else {
            Connection::open(db_path)?
        };
        
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        
        // Sort sheets by dependency (migrate dependencies first)
        let ordered_sheets = Self::order_sheets_by_dependency(&sheets);
        
        // Migrate each sheet
        for (idx, sheet_name) in ordered_sheets.iter().enumerate() {
            if let Some(pair) = sheets.get(sheet_name) {
                match Self::migrate_sheet_from_json(
                    &mut conn,
                    &pair.data_path,
                    &pair.meta_path,
                    sheet_name,
                    Some(idx as i32),
                    None,
                ) {
                    Ok(_) => {
                        report.sheets_migrated += 1;
                        for dep in &pair.dependencies {
                            if !report.linked_sheets_found.contains(dep) {
                                report.linked_sheets_found.push(dep.clone());
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to migrate '{}': {}", sheet_name, e);
                        report.sheets_failed += 1;
                        report.failed_sheets.push((sheet_name.clone(), e.to_string()));
                    }
                }
            }
        }
        
        Ok(report)
    }
    
    /// Order sheets so dependencies are migrated first
    pub fn order_sheets_by_dependency(sheets: &HashMap<String, JsonSheetPair>) -> Vec<String> {
        let mut ordered = Vec::new();
        let mut visited = HashSet::new();
        
        fn visit(
            name: &str,
            sheets: &HashMap<String, JsonSheetPair>,
            visited: &mut HashSet<String>,
            ordered: &mut Vec<String>,
        ) {
            if visited.contains(name) {
                return;
            }
            
            visited.insert(name.to_string());
            
            if let Some(pair) = sheets.get(name) {
                // Visit dependencies first
                for dep in &pair.dependencies {
                    if sheets.contains_key(dep) {
                        visit(dep, sheets, visited, ordered);
                    }
                }
            }
            
            ordered.push(name.to_string());
        }
        
        for name in sheets.keys() {
            visit(name, sheets, &mut visited, &mut ordered);
        }
        
        ordered
    }
    
    /// Export sheet from database to JSON
    pub fn export_sheet_to_json(
        conn: &Connection,
        table_name: &str,
        output_folder: &Path,
    ) -> DbResult<()> {
        use super::reader::DbReader;
        
        let sheet_data = DbReader::read_sheet(conn, table_name)?;
        
        let metadata = sheet_data.metadata
            .ok_or_else(|| DbError::InvalidMetadata("No metadata found".into()))?;
        
        // Write data file
        let data_path = output_folder.join(format!("{}.json", table_name));
        let data_json = serde_json::to_string_pretty(&sheet_data.grid)?;
        std::fs::write(data_path, data_json)?;
        
        // Write metadata file
        let meta_path = output_folder.join(format!("{}.meta.json", table_name));
        let meta_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(meta_path, meta_json)?;
        
        info!("Exported '{}' to JSON", table_name);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct JsonSheetPair {
    pub name: String,
    pub data_path: PathBuf,
    pub meta_path: PathBuf,
    pub dependencies: Vec<String>,
    pub category: Option<String>,
}
