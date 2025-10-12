// src/sheets/database/writer/renames.rs
// Rename operations - renaming columns and tables

use super::super::error::DbResult;
use rusqlite::{params, Connection};

/// Rename a data column and update its metadata column_name accordingly (for main or structure tables with real columns).
pub fn rename_data_column(
    conn: &Connection,
    table_name: &str,
    old_name: &str,
    new_name: &str,
) -> DbResult<()> {
    // Rename column in the data table
    conn.execute(
        &format!(
            "ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"",
            table_name, old_name, new_name
        ),
        [],
    )?;
    // Update metadata row
    let meta_table = format!("{}_Metadata", table_name);
    conn.execute(
        &format!(
            "UPDATE \"{}\" SET column_name = ? WHERE column_name = ?",
            meta_table
        ),
        params![new_name, old_name],
    )?;
    Ok(())
}

/// Update metadata column_name only (for columns that don't exist physically in main table, e.g., Structure validators)
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_metadata_column_name(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    new_name: &str,
) -> DbResult<()> {
    // Convert runtime column index to persisted index
    use rusqlite::OptionalExtension;
    let table_type: Option<String> = conn
        .query_row(
            "SELECT table_type FROM _Metadata WHERE table_name = ?",
            [table_name],
            |row| row.get(0),
        )
        .optional()?;
    
    let is_structure = matches!(table_type.as_deref(), Some("structure"));
    let persisted_index = if is_structure {
        if column_index < 2 {
            bevy::log::warn!(
                "Attempted to rename technical column {} in structure table '{}'",
                column_index,
                table_name
            );
            return Ok(());
        }
        column_index - 2
    } else {
        if column_index == 0 {
            bevy::log::warn!(
                "Attempted to rename technical column 0 (row_index) in regular table '{}'",
                table_name
            );
            return Ok(());
        }
        column_index - 1
    };
    
    let meta_table = format!("{}_Metadata", table_name);
    bevy::log::info!(
        "SQL update_metadata_column_name: table='{}' runtime_idx={} -> persisted_idx={} new_name='{}'",
        table_name, column_index, persisted_index, new_name
    );
    conn.execute(
        &format!(
            "UPDATE \"{}\" SET column_name = ? WHERE column_index = ?",
            meta_table
        ),
        params![new_name, persisted_index as i32],
    )?;
    Ok(())
}

/// Rename a structure table (and its metadata table); also fix _Metadata entries: table_name and parent_column.
pub fn rename_structure_table(
    conn: &Connection,
    parent_table: &str,
    old_column_name: &str,
    new_column_name: &str,
) -> DbResult<()> {
    let old_struct = format!("{}_{}", parent_table, old_column_name);
    let new_struct = format!("{}_{}", parent_table, new_column_name);
    // Rename data table
    conn.execute(
        &format!(
            "ALTER TABLE \"{}\" RENAME TO \"{}\"",
            old_struct, new_struct
        ),
        [],
    )?;
    // Rename metadata table
    let old_meta = format!("{}_Metadata", old_struct);
    let new_meta = format!("{}_Metadata", new_struct);
    conn.execute(
        &format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", old_meta, new_meta),
        [],
    )?;
    // Update global _Metadata entry for the structure table
    conn.execute(
        "UPDATE _Metadata SET table_name = ?, parent_column = ?, updated_at = CURRENT_TIMESTAMP WHERE table_name = ?",
        params![&new_struct, new_column_name, &old_struct],
    )?;
    Ok(())
}
