// src/sheets/database/reader/metadata_creation.rs
// Creation of metadata tables from physical database schema

use super::super::error::DbResult;
use super::super::schema::{create_metadata_table, sql_type_to_column_data_type};
use super::queries;
use crate::sheets::definitions::{ColumnDefinition, SheetMetadata};
use rusqlite::Connection;

/// Create metadata table from physical table schema
/// Used when a table exists but has no _Metadata table
pub fn create_metadata_from_physical_table(
    conn: &Connection,
    table_name: &str,
    daemon_client: &super::super::daemon_client::DaemonClient,
    db_name: Option<&str>,
) -> DbResult<()> {
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

    create_metadata_table(table_name, &sheet_meta, daemon_client, db_name)?;
    
    // Checkpoint WAL so the reader connection can see the newly created table
    let _ = conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(()))
        .map_err(|e| bevy::log::warn!("Failed to checkpoint WAL after metadata creation: {}", e));
    
    Ok(())
}
