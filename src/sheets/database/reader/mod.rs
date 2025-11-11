// src/sheets/database/reader/mod.rs
pub mod queries;
mod column_parser;

use super::error::DbResult;
use super::schema::{
    is_technical_column, sql_type_to_column_data_type,
};
use crate::sheets::definitions::{
    ColumnDataType, ColumnDefinition, ColumnValidator, SheetGridData, SheetMetadata,
};
use rusqlite::Connection;

pub struct DbReader;

impl DbReader {
    /// Read sheet metadata from database
    pub fn read_metadata(conn: &Connection, table_name: &str, daemon_client: &super::daemon_client::DaemonClient) -> DbResult<SheetMetadata> {
        let meta_table = format!("{}_Metadata", table_name);

        // Ensure metadata table exists
        let metadata_table_exists = super::schema::queries::table_exists(conn, &meta_table)?;
        bevy::log::debug!(
            "read_metadata: table_name='{}', meta_table='{}', exists={}",
            table_name, meta_table, metadata_table_exists
        );
        
        let freshly_created = if !metadata_table_exists {
            Self::create_metadata_from_physical_table(conn, table_name, daemon_client)?;
            true
        } else {
            false
        };

        // NOTE: These add_column_if_missing calls are ARCHITECTURE VIOLATIONS
        // Readers should not write! This needs to be fixed in a separate refactor.
        // For now, these go through daemon (proper write path)
        // If daemon is unavailable, we continue anyway since these are optional migrations
        // Skip adding columns to freshly-created tables (they already have the latest schema)
        if metadata_table_exists && !freshly_created {
            if let Err(e) = queries::add_column_if_missing(daemon_client, &meta_table, "deleted", "INTEGER", "0") {
                bevy::log::debug!("Could not add 'deleted' column to '{}': {}. Continuing anyway.", meta_table, e);
            }
            if let Err(e) = queries::add_column_if_missing(daemon_client, &meta_table, "display_name", "TEXT", "NULL") {
                bevy::log::debug!("Could not add 'display_name' column to '{}': {}. Continuing anyway.", meta_table, e);
            }
        }

        let table_type = super::schema::queries::get_table_type(conn, table_name)?;
        let is_structure = matches!(table_type.as_deref(), Some("structure"));

        // Read column definitions from metadata table
        let meta_rows = queries::read_metadata_columns(conn, &meta_table)?;
        
        // Validate physical/metadata alignment (diagnostic)
        if !meta_rows.is_empty() {
            Self::validate_physical_metadata_alignment(conn, table_name, &meta_rows)?;
        }
        bevy::log::info!(
            "read_metadata: '{}' -> {} metadata rows from {}_Metadata",
            table_name,
            meta_rows.len(),
            table_name
        );

        // Convert to ColumnDefinition objects
        let mut columns = column_parser::parse_metadata_columns(meta_rows)?;

        // Prepend technical columns based on table type
        columns = Self::prepend_technical_columns(columns, is_structure)?;

        // Auto-recover orphaned columns
        columns = Self::recover_orphaned_columns(conn, table_name, &meta_table, columns, daemon_client)?;

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

    pub fn read_sheet(conn: &Connection, table_name: &str, daemon_client: &super::daemon_client::DaemonClient) -> DbResult<SheetGridData> {
        let metadata = Self::read_metadata(conn, table_name, daemon_client)?;
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

    /// Validate that metadata columns have corresponding physical columns (diagnostic)
    /// Uses get_physical_index to verify alignment between metadata and actual DB schema
    fn validate_physical_metadata_alignment(
        conn: &Connection,
        table_name: &str,
        meta_rows: &[queries::MetadataColumnRow],
    ) -> DbResult<()> {
        let mut misaligned = Vec::new();
        
        for meta_row in meta_rows {
            // Skip deleted columns
            if matches!(meta_row.deleted, Some(1)) {
                continue;
            }
            
            // Check if this column has a physical counterpart
            match meta_row.get_physical_index(conn, table_name) {
                Ok(Some(phys_idx)) => {
                    // Column exists in physical schema - validate position
                    bevy::log::trace!(
                        "Column '{}' (metadata idx: {}) found at physical position {}",
                        meta_row.column_name,
                        meta_row.column_index,
                        phys_idx
                    );
                }
                Ok(None) => {
                    // Column exists in metadata but not in physical schema
                    // This is OK for Structure columns which don't have physical columns
                    bevy::log::trace!(
                        "Column '{}' (metadata idx: {}) has no physical column (likely Structure type)",
                        meta_row.column_name,
                        meta_row.column_index
                    );
                }
                Err(e) => {
                    bevy::log::warn!(
                        "Failed to check physical index for column '{}': {}",
                        meta_row.column_name,
                        e
                    );
                    misaligned.push(meta_row.column_name.clone());
                }
            }
        }
        
        if !misaligned.is_empty() {
            bevy::log::warn!(
                "Table '{}' has {} metadata columns that couldn't be validated: {:?}",
                table_name,
                misaligned.len(),
                misaligned
            );
        }
        
        Ok(())
    }

    fn create_metadata_from_physical_table(
        conn: &Connection,
        table_name: &str,
        daemon_client: &super::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        use crate::sheets::database::schema::create_metadata_table;

        bevy::log::warn!(
            "Metadata table for '{}' doesn't exist. Creating from physical schema...",
            table_name
        );

        let physical_cols = queries::get_physical_columns(conn, table_name)?;
        let mut columns = Vec::new();

        for (name, type_str) in physical_cols {
            let data_type = sql_type_to_column_data_type(&type_str);

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

        create_metadata_table(table_name, &sheet_meta, daemon_client)?;
        Ok(())
    }

    fn prepend_technical_columns(
        mut columns: Vec<ColumnDefinition>,
        is_structure: bool,
    ) -> DbResult<Vec<ColumnDefinition>> {
        // Filter out technical columns from persisted metadata
        // NOTE: grand_N_parent columns are NO LONGER technical columns - they are regular persisted data
        columns.retain(|c| {
            !is_technical_column(&c.header)
                && c.header != "temp_new_row_index"
                && c.header != "_obsolete_temp_new_row_index"
        });

        if is_structure {
            // Structure tables have exactly 2 technical columns: row_index and parent_key
            // grand_N_parent columns (if they exist) are now treated as regular data columns
            let mut with_tech = Vec::with_capacity(columns.len() + 2);

            // 1. row_index (hidden by default)
            with_tech.push(Self::create_technical_column(
                "row_index",
                ColumnDataType::I64,
                true,
            ));

            // 2. parent_key (visible as green read-only)
            with_tech.push(Self::create_technical_column(
                "parent_key",
                ColumnDataType::String,
                false,
            ));

            // 3. Data columns (including grand_N_parent if they exist as legacy columns)
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
        let mut col = ColumnDefinition::new_basic(name.to_string(), data_type);
        col.hidden = hidden;
        col
    }

    fn recover_orphaned_columns(
        conn: &Connection,
        table_name: &str,
        meta_table: &str,
        mut columns: Vec<ColumnDefinition>,
        daemon_client: &super::daemon_client::DaemonClient,
    ) -> DbResult<Vec<ColumnDefinition>> {
        let physical_columns = queries::get_physical_columns(conn, table_name)?;

        // Find orphaned columns (skip technical/system columns)
        let orphaned: Vec<(String, String)> = physical_columns
            .iter()
            .filter(|(phys_col, _)| {
                // Skip system columns
                if is_technical_column(phys_col)
                    || phys_col == "parent_id"
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
            let data_type = sql_type_to_column_data_type(sql_type);

            let insert_index = next_index + idx as i32;
            
            // Get physical position for diagnostic purposes
            let physical_position = queries::get_physical_column_names(conn, table_name)
                .ok()
                .and_then(|cols| cols.iter().position(|c| c.eq_ignore_ascii_case(col_name)));

            match queries::insert_orphaned_column_metadata(
                daemon_client,
                meta_table,
                insert_index,
                col_name,
                &format!("{:?}", data_type),
            ) {
                Ok(_) => {
                    if let Some(phys_idx) = physical_position {
                        bevy::log::info!(
                            "  ✓ Recovered '{}' as {:?} at metadata index {} (physical position: {})",
                            col_name,
                            data_type,
                            insert_index,
                            phys_idx
                        );
                    } else {
                        bevy::log::info!(
                            "  ✓ Recovered '{}' as {:?} at metadata index {}",
                            col_name,
                            data_type,
                            insert_index
                        );
                    }

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
            ai_model_id: table_meta.ai_model_id.unwrap_or_else(|| {
                crate::sheets::definitions::default_ai_model_id()
            }),
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