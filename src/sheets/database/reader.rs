// src/sheets/database/reader.rs

use super::error::{DbError, DbResult};
use crate::sheets::definitions::{
    ColumnDataType, ColumnDefinition, ColumnValidator, SheetGridData, SheetMetadata,
};
use rusqlite::Connection;
// use std::collections::HashMap;

pub struct DbReader;

impl DbReader {
    /// Read sheet metadata from database
    pub fn read_metadata(conn: &Connection, table_name: &str) -> DbResult<SheetMetadata> {
        let meta_table = format!("{}_Metadata", table_name);

        // Check if metadata table exists
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            [&meta_table],
            |row| row.get::<_, i32>(0).map(|v| v > 0),
        )?;

        if !exists {
            // Migration: legacy DB without metadata table - create metadata table from physical columns
            use crate::sheets::definitions::{ColumnDefinition, ColumnDataType, SheetMetadata};
            use crate::sheets::database::schema::create_metadata_table;
            // Build metadata column defs from PRAGMA table_info
            let mut columns_meta: Vec<ColumnDefinition> = Vec::new();
            let mut info = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
            for row in info.query_map([], |r| {
                let name: String = r.get(1)?;
                let type_str: String = r.get(2)?;
                let data_type = match type_str.as_str() {
                    "TEXT" => ColumnDataType::String,
                    "INTEGER" => ColumnDataType::I64,
                    "REAL" => ColumnDataType::F64,
                    _ => ColumnDataType::String,
                };
                Ok(ColumnDefinition {
                    header: name,
                    validator: None,
                    data_type,
                    filter: None,
                    ai_context: None,
                    ai_enable_row_generation: None,
                    ai_include_in_send: None,
                    width: None,
                    structure_schema: None,
                    structure_column_order: None,
                    structure_key_parent_column_index: None,
                    structure_ancestor_key_parent_column_indices: None,
                    deleted: false,
                    hidden: false,
                })
            })? {
                columns_meta.push(row?);
            }
            // Create new metadata table populated with existing columns
            let sheet_meta = SheetMetadata {
                sheet_name: table_name.to_string(),
                category: None,
                data_filename: format!("{}.json", table_name),
                columns: columns_meta,
                ai_general_rule: None,
                ai_model_id: crate::sheets::definitions::default_ai_model_id(),
                ai_temperature: None,
                ai_top_k: None,
                ai_top_p: None,
                requested_grounding_with_google_search: crate::sheets::definitions::default_grounding_with_google_search(),
                ai_enable_row_generation: false,
                ai_schema_groups: Vec::new(),
                ai_active_schema_group: None,
                random_picker: None,
                structure_parent: None,
                hidden: false,
            };
            create_metadata_table(conn, table_name, &sheet_meta)?;
        }
        // Migration: ensure 'deleted' column exists for metadata table in legacy DBs
        {
            let mut has_deleted = false;
            let mut info_stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", meta_table))?;
            for row in info_stmt.query_map([], |r| r.get::<_, String>(1))? {
                if row? == "deleted" {
                    has_deleted = true;
                    break;
                }
            }
            if !has_deleted {
                conn.execute(&format!("ALTER TABLE \"{}\" ADD COLUMN deleted INTEGER DEFAULT 0", meta_table), [])?;
            }
        }

        // Determine table type (main or structure)
        let table_type: Option<String> = conn
            .query_row(
                "SELECT table_type FROM _Metadata WHERE table_name = ?",
                [table_name],
                |row| row.get(0),
            )
            .ok();

        // Read column definitions
        // Include 'deleted' flag to hide deleted columns
        let mut stmt = conn.prepare(&format!(
            "SELECT column_index, column_name, data_type, validator_type, validator_config, ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send, deleted
             FROM \"{}\" ORDER BY column_index",
            meta_table
        ))?;
        bevy::log::info!("Prepared read_metadata SQL for '{}': SELECT ... FROM {}", table_name, meta_table);

        let columns: Vec<ColumnDefinition> = stmt
            .query_map([], |row| {
                let data_type_str: String = row.get(2)?;
                let data_type = match data_type_str.as_str() {
                    "String" => ColumnDataType::String,
                    "Bool" => ColumnDataType::Bool,
                    "I64" => ColumnDataType::I64,
                    "F64" => ColumnDataType::F64,
                    _ => ColumnDataType::String,
                };

                let validator_type: Option<String> = row.get(3)?;
                let validator_config: Option<String> = row.get(4)?;

                let validator = match validator_type.as_deref() {
                    Some("Basic") => Some(ColumnValidator::Basic(data_type)),
                    Some("Linked") => {
                        if let Some(config_json) = validator_config {
                            let config: serde_json::Value = serde_json::from_str(&config_json)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?;
                            Some(ColumnValidator::Linked {
                                target_sheet_name: config["target_table"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_string(),
                                target_column_index: config["target_column_index"]
                                    .as_u64()
                                    .unwrap_or(0)
                                    as usize,
                            })
                        } else {
                            None
                        }
                    }
                    Some("Structure") => Some(ColumnValidator::Structure),
                    _ => None,
                };

                let ai_enable_row_gen: Option<i32> = row.get(7).ok();
                let ai_include: Option<i32> = row.get(8).ok();
                // Deleted flag: hide columns marked deleted
                let deleted_flag: Option<i32> = row.get(9).ok();

                Ok(ColumnDefinition {
                    header: row.get(1)?,
                    validator,
                    data_type,
                    filter: row.get(6)?,
                    ai_context: row.get(5)?,
                    ai_enable_row_generation: ai_enable_row_gen.map(|v| v != 0),
                    ai_include_in_send: ai_include.map(|v| v != 0),
                    width: None,
                    structure_schema: None,
                    structure_column_order: None,
                    structure_key_parent_column_index: None,
                    structure_ancestor_key_parent_column_indices: None,
                    deleted: deleted_flag.map(|v| v != 0).unwrap_or(false),
                    hidden: false, // Loaded from DB, not a runtime technical column
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Log current metadata column headers read from the <table>_Metadata table
        let meta_headers: Vec<String> = columns.iter().map(|c| c.header.clone()).collect();
        bevy::log::info!(
            "read_metadata: '{}' -> metadata rows read = {} ; headers = {:?}",
            table_name,
            meta_headers.len(),
            meta_headers
        );

        // Prepend technical columns for structure tables (id, parent_key, row_index)
        // For structure tables, we always prepend technical columns.
        // If they already exist in persisted metadata (from older versions), remove them first.
        let columns = if matches!(table_type.as_deref(), Some("structure")) {
            // Filter out any existing technical columns from persisted metadata
            let original_len = columns.len();
            let filtered: Vec<ColumnDefinition> = columns
                .into_iter()
                .filter(|c| c.header != "id" && c.header != "parent_key" && c.header != "row_index")
                .collect();
            
            if filtered.len() < original_len {
                bevy::log::warn!(
                    "read_metadata: structure table '{}' had technical columns in persisted metadata (removed and will re-prepend)",
                    table_name
                );
            }
            
            // Always prepend technical columns in order: row_index (0), parent_key (1)
            // Note: 'id' is not prepended as it's redundant with row_index for user visibility
            // - row_index: hidden (internal-only, used for sorting/indexing)
            // - parent_key: visible as read-only green text (users need to see parent relationships)
            let mut with_tech = Vec::with_capacity(filtered.len() + 2);
            with_tech.push(ColumnDefinition {
                header: "row_index".to_string(),
                validator: None,
                data_type: ColumnDataType::I64,
                filter: None,
                ai_context: None,
                ai_enable_row_generation: None,
                ai_include_in_send: None,
                width: None,
                structure_schema: None,
                structure_column_order: None,
                structure_key_parent_column_index: None,
                structure_ancestor_key_parent_column_indices: None,
                deleted: false,
                hidden: true, // Hidden - internal indexing column
            });
            with_tech.push(ColumnDefinition {
                header: "parent_key".to_string(),
                validator: None,
                data_type: ColumnDataType::String,
                filter: None,
                ai_context: None,
                ai_enable_row_generation: None,
                ai_include_in_send: None,
                width: None,
                structure_schema: None,
                structure_column_order: None,
                structure_key_parent_column_index: None,
                structure_ancestor_key_parent_column_indices: None,
                deleted: false,
                hidden: false, // Visible as read-only green text (see ui/common.rs col_index == 1 handling)
            });
            with_tech.extend(filtered);
            with_tech
        } else {
            columns
        };

        // Log final column headers after potential prepend
        let final_headers: Vec<String> = columns.iter().map(|c| c.header.clone()).collect();
        bevy::log::info!(
            "read_metadata: '{}' -> final metadata columns = {} ; headers = {:?}",
            table_name,
            final_headers.len(),
            final_headers
        );

        // Read table-level metadata
        let (ai_allow_add_rows, ai_context, ai_active_group, category, hidden, ai_grounding): (i32, Option<String>, Option<String>, Option<String>, Option<i32>, Option<i32>) = 
            conn.query_row(
                "SELECT ai_allow_add_rows, ai_table_context, ai_active_group, category, hidden, ai_grounding_with_google_search 
                 FROM _Metadata WHERE table_name = ?",
                [table_name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4).ok(), row.get(5).ok()))
            ).unwrap_or((0, None, None, None, None, None));

        Ok(SheetMetadata {
            sheet_name: table_name.to_string(),
            category,
            data_filename: format!("{}.json", table_name),
            columns,
            ai_general_rule: ai_context,
            ai_model_id: "gemini-flash-latest".to_string(),
            ai_temperature: None,
            ai_top_k: None,
            ai_top_p: None,
            requested_grounding_with_google_search: Some(ai_grounding.unwrap_or(0) != 0),
            ai_enable_row_generation: ai_allow_add_rows != 0,
            ai_schema_groups: Vec::new(), // TODO: Read from groups table
            ai_active_schema_group: ai_active_group,
            random_picker: None,
            structure_parent: None,
            hidden: hidden
                .map(|v| v != 0)
                .unwrap_or_else(|| matches!(table_type.as_deref(), Some("structure"))),
        })
    }

    /// Read grid data from database
    pub fn read_grid_data(
        conn: &Connection,
        table_name: &str,
        metadata: &SheetMetadata,
    ) -> DbResult<Vec<Vec<String>>> {
        // Detect if this is a structure table
        let table_type: Option<String> = conn
            .query_row(
                "SELECT table_type FROM _Metadata WHERE table_name = ?",
                [table_name],
                |row| row.get(0),
            )
            .ok();

        let is_structure = matches!(table_type.as_deref(), Some("structure"));

        // Now include ALL columns for main tables, but for structure columns we'll query their count
        let non_structure_cols: Vec<(usize, String)> = metadata
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| !matches!(c.validator, Some(ColumnValidator::Structure)))
            .map(|(idx, c)| (idx, c.header.clone()))
            .collect();

        let structure_cols: Vec<(usize, String)> = metadata
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c.validator, Some(ColumnValidator::Structure)))
            .map(|(idx, c)| (idx, c.header.clone()))
            .collect();

        if non_structure_cols.is_empty() && structure_cols.is_empty() {
            return Ok(Vec::new());
        }

        // Cast all values to TEXT to avoid rusqlite type mismatch when retrieving as String
        let select_cols = non_structure_cols
            .iter()
            .map(|(_, name)| format!("CAST(\"{}\" AS TEXT) AS \"{}\"", name, name))
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!(
            "SELECT id, {} FROM \"{}\" ORDER BY row_index DESC",
            select_cols, table_name
        );
        bevy::log::info!("Prepared read_grid_data SQL for '{}': {}", table_name, query);

        let mut stmt = conn.prepare(&query)?;
        // Log how many columns the prepared statement will return (useful to diagnose "Invalid column index")
        let stmt_col_count = stmt.column_count();
        bevy::log::info!(
            "read_grid_data: prepared stmt for '{}' has column_count = {} ; metadata.columns = {} ; non_structure_cols = {} ; structure_cols = {} ; is_structure = {}",
            table_name,
            stmt_col_count,
            metadata.columns.len(),
            non_structure_cols.len(),
            structure_cols.len(),
            is_structure
        );

        let rows = stmt
            .query_map([], |row| {
                let row_id: i64 = row.get(0)?;

                // Read non-structure columns
                let mut non_structure_values = Vec::new();
                let value_cols_count = non_structure_cols.len();
                
                // Ensure we don't try to read more columns than the statement returns
                if stmt_col_count > 0 {
                    let max_values = stmt_col_count.saturating_sub(1); // minus id
                    if value_cols_count > max_values {
                        bevy::log::warn!(
                            "read_grid_data: metadata expects {} value columns but statement returns {} (id excluded) for table '{}'. Truncating read to available columns.",
                            value_cols_count,
                            max_values,
                            table_name
                        );
                    }
                }
                
                // Read values (skip column 0 which is always 'id' in SELECT)
                let actual_count = value_cols_count.min(stmt_col_count.saturating_sub(1));
                for i in 0..actual_count {
                    let value: Option<String> = row.get(i + 1).unwrap_or(None); // +1 because id is first
                    non_structure_values.push(value.unwrap_or_default());
                }

                // For structure columns, query count from structure tables
                let mut structure_counts: Vec<(usize, String)> = Vec::new();
                for (col_idx, col_name) in &structure_cols {
                    let structure_table = format!("{}_{}", table_name, col_name);
                    let count: i64 = conn
                        .query_row(
                            &format!(
                                "SELECT COUNT(*) FROM \"{}\" WHERE parent_id = ?",
                                structure_table
                            ),
                            [row_id],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    // Display as human-friendly label
                    let label = if count == 1 {
                        "1 row".to_string()
                    } else {
                        format!("{} rows", count)
                    };
                    structure_counts.push((*col_idx, label));
                }

                // Merge all columns back in correct order
                let mut cells = vec![String::new(); metadata.columns.len()];
                
                // Map non-structure column values back to their positions
                for ((col_idx, _), value) in non_structure_cols.iter().zip(non_structure_values.iter()) {
                    cells[*col_idx] = value.clone();
                }
                for (col_idx, count_str) in structure_counts {
                    cells[col_idx] = count_str;
                }

                Ok(cells)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Read complete sheet (metadata + grid)
    pub fn read_sheet(conn: &Connection, table_name: &str) -> DbResult<SheetGridData> {
        let metadata = Self::read_metadata(conn, table_name)?;
        let grid = Self::read_grid_data(conn, table_name, &metadata)?;

        Ok(SheetGridData {
            metadata: Some(metadata),
            grid,
        })
    }

    /// List all sheets in database (including structure tables so they are visible by default)
    pub fn list_sheets(conn: &Connection) -> DbResult<Vec<String>> {
        let mut stmt = conn.prepare(
            "SELECT table_name FROM _Metadata 
             WHERE table_type IN ('main','structure') OR table_type IS NULL
             ORDER BY display_order, table_name",
        )?;

        let sheets = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        Ok(sheets)
    }
}
