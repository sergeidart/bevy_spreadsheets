// persistence.rs
// Database persistence logic for column validator updates

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
};
use bevy::prelude::*;

/// Persists a non-Structure validator to the database.
/// Returns true if persistence was attempted, false if skipped.
pub fn persist_non_structure_validator(
    category_opt: &Option<String>,
    physical_table_name: &str,
    column_name: &str,
    data_type: ColumnDataType,
    validator: &Option<ColumnValidator>,
    ai_include_in_send: Option<bool>,
    ai_enable_row_generation: Option<bool>,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) -> bool {
    // Only persist if this sheet belongs to a database category
    let Some(cat_str) = category_opt else {
        return false;
    };
    
    // Skip technical columns (row_index, parent_key) - they're not in metadata table
    if column_name.eq_ignore_ascii_case("row_index") 
        || column_name.eq_ignore_ascii_case("parent_key") 
    {
        return false;
    }
    
    // Persist by column name to avoid index mismatch
    if let Err(e) = crate::sheets::database::persist_column_validator_by_name(
        cat_str,
        physical_table_name,
        column_name,
        data_type,
        validator,
        ai_include_in_send,
        ai_enable_row_generation,
        daemon_client,
    ) {
        error!("Persist column validator failed: {}", e);
    }
    
    true
}

/// Persists a Structure validator to the database.
/// This should be called AFTER the structure table is created.
pub fn persist_structure_validator(
    category: &str,
    parent_sheet_name: &str,
    column_name: &str,
    data_type: ColumnDataType,
    ai_include_in_send: Option<bool>,
    ai_enable_row_generation: Option<bool>,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) -> Result<(), String> {
    info!("💾 Persisting Structure validator for parent column '{}'", column_name);
    
    crate::sheets::database::persist_column_validator_by_name(
        category,
        parent_sheet_name,
        column_name,
        data_type,
        &Some(ColumnValidator::Structure),
        ai_include_in_send,
        ai_enable_row_generation,
        daemon_client,
    )
    .map_err(|e| {
        error!("Failed to persist Structure validator for parent column: {}", e);
        format!("Failed to persist Structure validator: {}", e)
    })?;
    
    info!("✅ Successfully persisted Structure validator to database");
    Ok(())
}
