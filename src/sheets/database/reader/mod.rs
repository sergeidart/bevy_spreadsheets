// src/sheets/database/reader/mod.rs
mod queries;

use super::error::DbResult;
use crate::sheets::definitions::{
    ColumnDataType, ColumnDefinition, ColumnValidator, SheetGridData, SheetMetadata,
};
use rusqlite::Connection;

pub use queries::*;

pub struct DbReader;

impl DbReader {
    /// Read sheet metadata from database
    pub fn read_metadata(conn: &Connection, table_name: &str) -> DbResult<SheetMetadata> {
        let meta_table = format!("{}_Metadata", table_name);

        // Ensure metadata table exists
        if !queries::table_exists(conn, &meta_table)? {
            Self::create_metadata_from_physical_table(conn, table_name)?;
        }

        // Ensure 'deleted' and 'display_name' columns exist in metadata table
        queries::add_column_if_missing(conn, &meta_table, "deleted", "INTEGER", "0")?;
        queries::add_column_if_missing(conn, &meta_table, "display_name", "TEXT", "NULL")?;

        let table_type = queries::get_table_type(conn, table_name);
        let is_structure = matches!(table_type.as_deref(), Some("structure"));

        // Read column definitions from metadata table
        let meta_rows = queries::read_metadata_columns(conn, &meta_table)?;
        bevy::log::info!(
            "read_metadata: '{}' -> {} metadata rows from {}_Metadata",
            table_name,
            meta_rows.len(),
            table_name
        );

        // Convert to ColumnDefinition objects
        let mut columns = Self::parse_metadata_columns(meta_rows)?;

        // Prepend technical columns based on table type
        columns = Self::prepend_technical_columns(conn, table_name, columns, is_structure)?;

        // Auto-recover orphaned columns
        columns = Self::recover_orphaned_columns(conn, table_name, &meta_table, columns)?;

        // Read table-level metadata
        let table_meta = queries::read_table_metadata(conn, table_name)?;

        Ok(Self::build_sheet_metadata(
            table_name,
            columns,
            table_meta,
            is_structure,
        ))
    }

    /// Read grid data from database
    pub fn read_grid_data(
        conn: &Connection,
        table_name: &str,
        metadata: &SheetMetadata,
    ) -> DbResult<(Vec<Vec<String>>, Vec<i64>)> {
        // Separate structure and non-structure columns
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

        // Read grid rows with structure counts
        let grid_rows = queries::read_grid_with_structure_counts(
            conn,
            table_name,
            &non_structure_cols,
            &structure_cols,
        )?;

        // Build final grid and row_indices
        let mut grid = Vec::new();
        let mut row_indices = Vec::new();

        for row in grid_rows {
            let mut cells = vec![String::new(); metadata.columns.len()];

            // Fill non-structure values
            for ((col_idx, _), value) in non_structure_cols.iter().zip(row.values.iter()) {
                cells[*col_idx] = value.clone();
            }

            // Fill structure counts
            for (col_idx, count_str) in row.structure_counts {
                cells[col_idx] = count_str;
            }

            grid.push(cells);
            row_indices.push(row.row_index);
        }

        Ok((grid, row_indices))
    }

    pub fn read_sheet(conn: &Connection, table_name: &str) -> DbResult<SheetGridData> {
        let metadata = Self::read_metadata(conn, table_name)?;
        let (grid, row_indices) = Self::read_grid_data(conn, table_name, &metadata)?;

        Ok(SheetGridData {
            metadata: Some(metadata),
            grid,
            row_indices,
        })
    }

    pub fn list_sheets(conn: &Connection) -> DbResult<Vec<String>> {
        queries::list_all_tables(conn)
    }

    // ========================================================================
    // Private helper methods
    // ========================================================================

    fn create_metadata_from_physical_table(
        conn: &Connection,
        table_name: &str,
    ) -> DbResult<()> {
        use crate::sheets::database::schema::create_metadata_table;

        bevy::log::warn!(
            "Metadata table for '{}' doesn't exist. Creating from physical schema...",
            table_name
        );

        let physical_cols = queries::get_physical_columns(conn, table_name)?;
        let mut columns = Vec::new();

        for (name, type_str) in physical_cols {
            let data_type = match type_str.as_str() {
                "TEXT" => ColumnDataType::String,
                "INTEGER" => ColumnDataType::I64,
                "REAL" => ColumnDataType::F64,
                _ => ColumnDataType::String,
            };

            columns.push(ColumnDefinition {
                header: name,
                display_header: None,
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
            });
        }

        let sheet_meta = SheetMetadata {
            sheet_name: table_name.to_string(),
            category: None,
            data_filename: format!("{}.json", table_name),
            columns,
            ai_general_rule: None,
            ai_model_id: crate::sheets::definitions::default_ai_model_id(),
            ai_temperature: None,
            requested_grounding_with_google_search:
                crate::sheets::definitions::default_grounding_with_google_search(),
            ai_enable_row_generation: false,
            ai_schema_groups: Vec::new(),
            ai_active_schema_group: None,
            random_picker: None,
            structure_parent: None,
            hidden: false,
        };

        create_metadata_table(conn, table_name, &sheet_meta)?;
        Ok(())
    }

    fn parse_metadata_columns(
        meta_rows: Vec<queries::MetadataColumnRow>,
    ) -> DbResult<Vec<ColumnDefinition>> {
        let mut columns = Vec::new();

        for row in meta_rows {
            let data_type = match row.data_type.as_str() {
                "String" => ColumnDataType::String,
                "Bool" => ColumnDataType::Bool,
                "I64" => ColumnDataType::I64,
                "F64" => ColumnDataType::F64,
                _ => ColumnDataType::String,
            };

            let validator = match row.validator_type.as_deref() {
                Some("Basic") => Some(ColumnValidator::Basic(data_type)),
                Some("Linked") => {
                    if let Some(config_json) = row.validator_config {
                        let config: serde_json::Value =
                            serde_json::from_str(&config_json).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?;
                        Some(ColumnValidator::Linked {
                            target_sheet_name: config["target_table"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            target_column_index: config["target_column_index"]
                                .as_u64()
                                .unwrap_or(0) as usize,
                        })
                    } else {
                        None
                    }
                }
                Some("Structure") => Some(ColumnValidator::Structure),
                _ => None,
            };

            let deleted = row.deleted.map(|v| v != 0).unwrap_or(false);

            // Skip deleted columns
            if deleted {
                continue;
            }

            columns.push(ColumnDefinition {
                header: row.column_name,
                display_header: row.display_name,
                validator,
                data_type,
                filter: row.filter_expr,
                ai_context: row.ai_context,
                ai_enable_row_generation: row.ai_enable_row_generation.map(|v| v != 0),
                ai_include_in_send: row.ai_include_in_send.map(|v| v != 0),
                width: None,
                structure_schema: None,
                structure_column_order: None,
                structure_key_parent_column_index: None,
                structure_ancestor_key_parent_column_indices: None,
                deleted: false,
                hidden: false,
            });
        }

        Ok(columns)
    }

    fn prepend_technical_columns(
        conn: &Connection,
        table_name: &str,
        mut columns: Vec<ColumnDefinition>,
        is_structure: bool,
    ) -> DbResult<Vec<ColumnDefinition>> {
        // Filter out technical columns from persisted metadata
        columns.retain(|c| {
            c.header != "id"
                && c.header != "parent_key"
                && c.header != "row_index"
                && c.header != "temp_new_row_index"
                && c.header != "_obsolete_temp_new_row_index"
                && !(c.header.starts_with("grand_") && c.header.ends_with("_parent"))
        });

        if is_structure {
            // Get grand_N_parent columns from physical schema
            let db_columns = queries::get_physical_column_names(conn, table_name)?;
            let mut grand_parent_columns: Vec<String> = db_columns
                .iter()
                .filter(|name| name.starts_with("grand_") && name.ends_with("_parent"))
                .cloned()
                .collect();
            grand_parent_columns.sort_by(|a, b| b.cmp(a)); // Descending order

            let tech_col_count = 2 + grand_parent_columns.len();
            let mut with_tech = Vec::with_capacity(columns.len() + tech_col_count);

            // 1. row_index (hidden by default)
            with_tech.push(Self::create_technical_column(
                "row_index",
                ColumnDataType::I64,
                true,
            ));

            // 2. grand_N_parent columns (visible as green read-only)
            for grand_col_name in grand_parent_columns {
                with_tech.push(Self::create_technical_column(
                    &grand_col_name,
                    ColumnDataType::String,
                    false,
                ));
            }

            // 3. parent_key (visible as green read-only)
            with_tech.push(Self::create_technical_column(
                "parent_key",
                ColumnDataType::String,
                false,
            ));

            // 4. Data columns
            with_tech.extend(columns);
            Ok(with_tech)
        } else {
            // Regular table: just prepend row_index
            let mut with_row_index = Vec::with_capacity(columns.len() + 1);
            with_row_index.push(Self::create_technical_column(
                "row_index",
                ColumnDataType::I64,
                true,
            ));
            with_row_index.extend(columns);
            Ok(with_row_index)
        }
    }

    fn create_technical_column(
        name: &str,
        data_type: ColumnDataType,
        hidden: bool,
    ) -> ColumnDefinition {
        ColumnDefinition {
            header: name.to_string(),
            display_header: None,
            validator: Some(ColumnValidator::Basic(data_type)),
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
            hidden,
        }
    }

    fn recover_orphaned_columns(
        conn: &Connection,
        table_name: &str,
        meta_table: &str,
        mut columns: Vec<ColumnDefinition>,
    ) -> DbResult<Vec<ColumnDefinition>> {
        let physical_columns = queries::get_physical_columns(conn, table_name)?;

        // Find orphaned columns (skip technical/system columns)
        let orphaned: Vec<(String, String)> = physical_columns
            .iter()
            .filter(|(phys_col, _)| {
                // Skip system columns
                if phys_col == "id"
                    || phys_col == "parent_id"
                    || phys_col == "row_index"
                    || phys_col == "parent_key"
                    || phys_col == "temp_new_row_index"
                    || phys_col == "_obsolete_temp_new_row_index"
                    || phys_col == "created_at"
                    || phys_col == "updated_at"
                    || (phys_col.starts_with("grand_") && phys_col.ends_with("_parent"))
                {
                    return false;
                }

                // Check if exists in metadata
                !columns
                    .iter()
                    .any(|meta_col| meta_col.header.eq_ignore_ascii_case(phys_col))
            })
            .cloned()
            .collect();

        if orphaned.is_empty() {
            return Ok(columns);
        }

        bevy::log::warn!(
            "read_metadata: '{}' has {} orphaned columns: {:?}",
            table_name,
            orphaned.len(),
            orphaned.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        );

        // Find next available index by querying the max column_index from the database
        let next_index: i32 = conn
            .query_row(
                &format!("SELECT COALESCE(MAX(column_index), -1) + 1 FROM \"{}\"", meta_table),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        bevy::log::debug!(
            "Orphaned column recovery: next_index={} (from max in DB)",
            next_index
        );

        // Recover each orphaned column
        for (idx, (col_name, sql_type)) in orphaned.iter().enumerate() {
            let data_type = match sql_type.to_uppercase().as_str() {
                "INTEGER" => ColumnDataType::I64,
                "REAL" | "FLOAT" | "DOUBLE" => ColumnDataType::F64,
                _ => ColumnDataType::String,
            };

            let insert_index = next_index + idx as i32;

            match queries::insert_orphaned_column_metadata(
                conn,
                meta_table,
                insert_index,
                col_name,
                &format!("{:?}", data_type),
            ) {
                Ok(_) => {
                    bevy::log::info!(
                        "  ✓ Recovered '{}' as {:?} at index {}",
                        col_name,
                        data_type,
                        insert_index
                    );

                    columns.push(ColumnDefinition {
                        header: col_name.clone(),
                        display_header: None,
                        validator: Some(ColumnValidator::Basic(data_type)),
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
                    });
                }
                Err(e) => {
                    bevy::log::error!("  ✗ Failed to recover '{}': {}", col_name, e);
                }
            }
        }

        Ok(columns)
    }
    fn build_sheet_metadata(
        table_name: &str,
        columns: Vec<ColumnDefinition>,
        table_meta: queries::TableMetadataRow,
        is_structure: bool,
    ) -> SheetMetadata {
        SheetMetadata {
            sheet_name: table_name.to_string(),
            category: table_meta.category,
            data_filename: format!("{}.json", table_name),
            columns,
            ai_general_rule: table_meta.ai_table_context,
            ai_model_id: "gemini-flash-latest".to_string(),
            ai_temperature: None,
            requested_grounding_with_google_search: Some(
                table_meta.ai_grounding.unwrap_or(0) != 0,
            ),
            ai_enable_row_generation: table_meta.ai_allow_add_rows != 0,
            ai_schema_groups: Vec::new(), // TODO: Read from groups table
            ai_active_schema_group: table_meta.ai_active_group,
            random_picker: None,
            structure_parent: None,
            hidden: table_meta
                .hidden
                .map(|v| v != 0)
                .unwrap_or(is_structure),
        }
    }
}