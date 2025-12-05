// src/sheets/database/reader/structure_population.rs
// Population of structure schemas from child tables

use super::super::error::DbResult;
use crate::sheets::definitions::{ColumnDefinition, ColumnValidator};
use rusqlite::Connection;

/// Populate structure_schema from child tables for Structure columns
/// Structure schemas are not persisted in parent table metadata - they must be loaded from child tables
pub fn populate_structure_schemas_from_child_tables(
    conn: &Connection,
    parent_table_name: &str,
    mut columns: Vec<ColumnDefinition>,
    daemon_client: &super::super::daemon_client::DaemonClient,
    db_name: Option<&str>,
    read_metadata_fn: impl Fn(&Connection, &str, &super::super::daemon_client::DaemonClient, Option<&str>) -> DbResult<crate::sheets::definitions::SheetMetadata>,
) -> DbResult<Vec<ColumnDefinition>> {
    bevy::log::info!(
        "populate_structure_schemas: Starting for '{}' with {} columns",
        parent_table_name,
        columns.len()
    );
    
    for col in columns.iter_mut() {
        bevy::log::debug!(
            "  Column '{}': validator={:?}",
            col.header,
            col.validator
        );
        
        // Skip non-Structure columns
        if !matches!(col.validator, Some(ColumnValidator::Structure)) {
            continue;
        }

        // Build child table name: ParentTable_ColumnName
        let child_table_name = format!("{}_{}", parent_table_name, col.header);

        // Check if child table exists
        if !super::super::schema::queries::table_exists(conn, &child_table_name)? {
            bevy::log::warn!(
                "Structure column '{}' in '{}' has no child table '{}'  (legacy data not migrated)",
                col.header,
                parent_table_name,
                child_table_name
            );
            continue;
        }

        bevy::log::info!(
            "Found child table '{}' for Structure column '{}' in '{}'",
            child_table_name,
            col.header,
            parent_table_name
        );

        // Read child table metadata
        match read_metadata_fn(conn, &child_table_name, daemon_client, db_name) {
            Ok(child_metadata) => {
                // Convert child columns to structure fields
                let structure_fields: Vec<crate::sheets::structure_field::StructureFieldDefinition> = child_metadata
                    .columns
                    .iter()
                    .filter(|c| {
                        // Skip technical columns
                        !matches!(c.header.as_str(), "id" | "row_index" | "parent_key")
                    })
                    .map(|c| crate::sheets::structure_field::StructureFieldDefinition {
                        header: c.header.clone(),
                        data_type: c.data_type,
                        validator: c.validator.clone(),
                        filter: c.filter.clone(),
                        ai_context: c.ai_context.clone(),
                        ai_include_in_send: c.ai_include_in_send,
                        ai_enable_row_generation: c.ai_enable_row_generation,
                        width: c.width,
                        structure_schema: c.structure_schema.clone(),
                        structure_column_order: c.structure_column_order.clone(),
                        structure_key_parent_column_index: c.structure_key_parent_column_index,
                        structure_ancestor_key_parent_column_indices: c.structure_ancestor_key_parent_column_indices.clone(),
                    })
                    .collect();

                if !structure_fields.is_empty() {
                    bevy::log::info!(
                        "Populated structure_schema for column '{}' in '{}' from child table '{}' ({} fields)",
                        col.header,
                        parent_table_name,
                        child_table_name,
                        structure_fields.len()
                    );
                    col.structure_schema = Some(structure_fields);
                }
            }
            Err(e) => {
                bevy::log::warn!(
                    "Failed to read metadata for child table '{}': {}",
                    child_table_name,
                    e
                );
            }
        }
    }

    Ok(columns)
}
