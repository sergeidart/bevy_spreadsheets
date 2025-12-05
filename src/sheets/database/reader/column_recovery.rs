// src/sheets/database/reader/column_recovery.rs
// Auto-recovery of orphaned columns (physical columns without metadata entries)

use super::super::error::DbResult;
use super::super::schema::{is_technical_column, sql_type_to_column_data_type};
use super::queries;
use crate::sheets::definitions::{ColumnDefinition, ColumnValidator};
use rusqlite::Connection;

/// Recover orphaned columns (physical columns that exist but have no metadata)
/// Returns updated columns vector with recovered columns appended
pub fn recover_orphaned_columns(
    conn: &Connection,
    table_name: &str,
    meta_table: &str,
    mut columns: Vec<ColumnDefinition>,
    daemon_client: &super::super::daemon_client::DaemonClient,
    db_name: Option<&str>,
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
            db_name,
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
