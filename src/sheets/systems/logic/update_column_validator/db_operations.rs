// db_operations.rs
// Database operations for structure table creation and management

use crate::sheets::{
    definitions::{ColumnDefinition, SheetMetadata},
    resources::SheetRegistry,
    events::SheetDataModifiedInRegistryEvent,
};
use bevy::prelude::*;
use rusqlite::Connection;
use std::path::Path;

use super::content_copy::copy_parent_content_to_structure_table;
use super::persistence::persist_structure_validator;

/// Creates a DB structure table and handles all related operations.
/// Returns Ok(()) if successful, Err(message) if failed.
pub fn create_db_structure_table(
    conn: &Connection,
    cat_str: &str,
    struct_sheet_name: &str,
    parent_sheet_name: &str,
    parent_col_def: &ColumnDefinition,
    struct_columns: &[ColumnDefinition],
    structure_metadata: &SheetMetadata,
    db_path: &Path,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    registry: &mut SheetRegistry,
    data_modified_writer: &mut EventWriter<SheetDataModifiedInRegistryEvent>,
) -> Result<(), String> {
    info!("Creating DB table for structure sheet: {}", struct_sheet_name);
    
    // Log what we're passing
    info!(
        "About to create structure table with parent_col_def: header='{}', structure_schema.len()={}, structure_key_parent_column_index={:?}, struct_columns.len()={}",
        parent_col_def.header,
        parent_col_def.structure_schema.as_ref().map(|s| s.len()).unwrap_or(0),
        parent_col_def.structure_key_parent_column_index,
        struct_columns.len()
    );
    
    // Create the structure table with multi-level support
    crate::sheets::database::schema::create_structure_table(
        conn,
        parent_sheet_name,
        parent_col_def,
        Some(struct_columns),
        daemon_client,
        db_path.file_name().and_then(|n| n.to_str()),
    )
    .map_err(|e| format!("Failed to create structure table '{}': {}", struct_sheet_name, e))?;
    
    info!("Successfully created DB structure table: {}", struct_sheet_name);
    
    // Create metadata table for the structure sheet
    if let Err(e) = crate::sheets::database::schema::create_metadata_table(
        struct_sheet_name,
        structure_metadata,
        daemon_client,
        db_path.file_name().and_then(|n| n.to_str()),
    ) {
        warn!(
            "Failed to create metadata table for structure '{}': {} (will continue)",
            struct_sheet_name,
            e
        );
    }
    
    // Best-effort cleanup: drop the parent table's physical column if it existed
    let _ = crate::sheets::database::writer::DbWriter::drop_physical_column_if_exists(
        conn,
        parent_sheet_name,
        &parent_col_def.header,
        db_path.file_name().and_then(|n| n.to_str()),
        daemon_client,
    );
    
    // Copy content from parent table to structure table
    copy_parent_content_to_structure_table(
        conn,
        parent_sheet_name,
        struct_sheet_name,
        parent_col_def,
        struct_columns,
        db_path,
        daemon_client,
    )
    .map_err(|e| format!("Failed to copy content to structure table '{}': {}", struct_sheet_name, e))?;
    
    info!("Successfully copied content to structure table: {}", struct_sheet_name);
    
    // Persist the parent column's Structure validator to database
    persist_structure_validator(
        cat_str,
        parent_sheet_name,
        &parent_col_def.header,
        parent_col_def.data_type,
        parent_col_def.ai_include_in_send,
        parent_col_def.ai_enable_row_generation,
        daemon_client,
    )?;
    
    // Reload parent sheet from DB to sync in-memory state with database
    reload_and_restore_parent_sheet(
        conn,
        parent_sheet_name,
        parent_col_def,
        db_path,
        daemon_client,
        registry,
        &parent_col_def.header.clone(),
        data_modified_writer,
        cat_str,
    )?;
    
    Ok(())
}

/// Reloads parent sheet from database and restores structure_schema fields.
/// These fields are not persisted to DB, so they must be manually restored.
fn reload_and_restore_parent_sheet(
    conn: &Connection,
    parent_sheet_name: &str,
    parent_col_def: &ColumnDefinition,
    db_path: &Path,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    registry: &mut SheetRegistry,
    column_header: &str,
    data_modified_writer: &mut EventWriter<SheetDataModifiedInRegistryEvent>,
    category: &str,
) -> Result<(), String> {
    info!("🔄 Reloading parent sheet '{}' from database after structure table creation", parent_sheet_name);
    
    let mut reloaded_parent = crate::sheets::database::reader::DbReader::read_sheet(
        conn,
        parent_sheet_name,
        daemon_client,
        db_path.file_name().and_then(|n| n.to_str()),
    )
    .map_err(|e| {
        error!("❌ Failed to reload parent sheet '{}' from database: {}", parent_sheet_name, e);
        format!("Failed to reload parent sheet: {}", e)
    })?;
    
    // Restore structure_schema fields from parent_col_def
    if let Some(ref mut meta) = reloaded_parent.metadata {
        if let Some((col_idx, _)) = meta.columns.iter().enumerate()
            .find(|(_, col)| col.header == column_header)
        {
            info!("🔧 Restoring structure_schema fields for column '{}'", column_header);
            meta.columns[col_idx].structure_schema = parent_col_def.structure_schema.clone();
            meta.columns[col_idx].structure_column_order = parent_col_def.structure_column_order.clone();
            meta.columns[col_idx].structure_key_parent_column_index = parent_col_def.structure_key_parent_column_index;
            info!("✅ Structure schema fields restored: schema_len={}, order_len={}, key_parent_idx={:?}",
                parent_col_def.structure_schema.as_ref().map(|s| s.len()).unwrap_or(0),
                parent_col_def.structure_column_order.as_ref().map(|o| o.len()).unwrap_or(0),
                parent_col_def.structure_key_parent_column_index
            );
        }
    }
    
    registry.add_or_replace_sheet(
        Some(category.to_string()),
        parent_sheet_name.to_string(),
        reloaded_parent,
    );
    
    info!("✅ Successfully reloaded parent sheet '{}' - in-memory state now matches database", parent_sheet_name);
    
    // Emit data modified event for parent sheet so UI refreshes
    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
        category: Some(category.to_string()),
        sheet_name: parent_sheet_name.to_string(),
    });
    
    Ok(())
}
