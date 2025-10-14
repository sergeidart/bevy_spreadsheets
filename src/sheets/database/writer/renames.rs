// src/sheets/database/writer/renames.rs
// Rename operations - renaming columns and tables

use super::super::error::DbResult;
use rusqlite::{params, Connection, OptionalExtension};

/// Rename a data column and update its metadata column_name accordingly (for main or structure tables with real columns).
pub fn rename_data_column(
    conn: &Connection,
    table_name: &str,
    old_name: &str,
    new_name: &str,
) -> DbResult<()> {
    // Check if a column with new_name already exists in the DB schema
    let mut pragma_stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    let existing_columns: Vec<String> = pragma_stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    
    bevy::log::debug!("rename_data_column: table='{}', old='{}', new='{}', existing_cols={:?}", 
        table_name, old_name, new_name, existing_columns);
    
    // Check if old_name exists physically in the table
    let old_name_exists = existing_columns.iter()
        .any(|col| col.eq_ignore_ascii_case(old_name));
    
    // Check if new_name already exists (case-insensitive)
    let new_name_exists = existing_columns.iter()
        .any(|col| col.eq_ignore_ascii_case(new_name));
    
    if new_name_exists {
        // Check if it's marked as deleted in metadata
        let meta_table = format!("{}_Metadata", table_name);
        let is_deleted: Option<i32> = conn.query_row(
            &format!("SELECT deleted FROM \"{}\" WHERE column_name = ?", meta_table),
            params![new_name],
            |row| row.get(0),
        ).ok();
        
        if matches!(is_deleted, Some(1)) {
            bevy::log::info!("Column '{}' exists in DB but is marked deleted - dropping it first", new_name);
            // Drop the deleted column from the table
            // SQLite doesn't support DROP COLUMN directly in old versions, so we need to check
            // Try to drop it (SQLite 3.35.0+ supports ALTER TABLE DROP COLUMN)
            match conn.execute(
                &format!("ALTER TABLE \"{}\" DROP COLUMN \"{}\"", table_name, new_name),
                [],
            ) {
                Ok(_) => {
                    bevy::log::info!("Successfully dropped deleted column '{}'", new_name);
                    // Also remove from metadata
                    conn.execute(
                        &format!("DELETE FROM \"{}\" WHERE column_name = ?", meta_table),
                        params![new_name],
                    )?;
                }
                Err(e) => {
                    bevy::log::warn!("Failed to drop column '{}': {} - SQLite version may not support DROP COLUMN. Trying workaround.", new_name, e);
                    // Workaround: Just delete from metadata and mark it as renamed
                    conn.execute(
                        &format!("DELETE FROM \"{}\" WHERE column_name = ?", meta_table),
                        params![new_name],
                    )?;
                    // Note: The physical column will remain in DB but won't be in metadata
                }
            }
        } else {
            return Err(super::super::error::DbError::Other(format!(
                "Column '{}' already exists in table '{}' and is not marked as deleted",
                new_name, table_name
            )));
        }
    }
    
    // Check if old column exists physically - if not, only update metadata
    if !old_name_exists {
        bevy::log::info!(
            "Column '{}' does not exist physically in table '{}' - only updating metadata",
            old_name, table_name
        );
        
        // Just update metadata - need to handle deleted columns with the new name
        let meta_table = format!("{}_Metadata", table_name);
        
        // First, get the column_index of the column we're renaming
        let source_column_index: Option<i32> = conn
            .query_row(
                &format!(
                    "SELECT column_index FROM \"{}\" WHERE column_name = ?",
                    meta_table
                ),
                params![old_name],
                |row| row.get(0),
            )
            .optional()?;
        
        let source_idx = match source_column_index {
            Some(idx) => idx,
            None => {
                bevy::log::error!(
                    "rename_data_column: Column '{}' not found in metadata table '{}'",
                    old_name, meta_table
                );
                return Err(super::super::error::DbError::Other(format!(
                    "Column '{}' not found in metadata table '{}'",
                    old_name, meta_table
                )));
            }
        };
        
        bevy::log::debug!(
            "rename_data_column (metadata-only): source column '{}' has index {}",
            old_name, source_idx
        );
        
        // Check if ANY column with the new name exists at a DIFFERENT index
        let conflicting_row: Option<(i32, i32)> = conn
            .query_row(
                &format!(
                    "SELECT column_index, deleted FROM \"{}\" WHERE column_name = ?",
                    meta_table
                ),
                params![new_name],
                |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?)),
            )
            .optional()?;
        
        bevy::log::debug!(
            "rename_data_column (metadata-only): conflicting row check for '{}' returned {:?}",
            new_name, conflicting_row
        );
        
        if let Some((existing_idx, is_deleted)) = conflicting_row {
            if existing_idx != source_idx {
                // There's a column with this name at a different index
                if is_deleted == 1 {
                    // Delete the conflicting deleted metadata row to allow reuse of the name
                    bevy::log::warn!(
                        "Found deleted column '{}' at index {} in '{}' - deleting its metadata row to avoid conflict (source index={})",
                        new_name, existing_idx, meta_table, source_idx
                    );
                    conn.execute(
                        &format!(
                            "DELETE FROM \"{}\" WHERE column_name = ? AND deleted = 1",
                            meta_table
                        ),
                        params![new_name],
                    )?;
                } else {
                    // Active column with same name at different index - this is an error
                    return Err(super::super::error::DbError::Other(format!(
                        "Column '{}' already exists at index {} in table '{}' (not deleted)",
                        new_name, existing_idx, table_name
                    )));
                }
            }
            // If existing_idx == source_idx, that means the column is already named this way (no-op)
        }
        
        // Update metadata - use column_index to be precise
        bevy::log::debug!(
            "rename_data_column (metadata-only): executing UPDATE \"{}\" SET column_name = '{}' WHERE column_index = {}",
            meta_table, new_name, source_idx
        );
        
        match conn.execute(
            &format!(
                "UPDATE \"{}\" SET column_name = ? WHERE column_index = ?",
                meta_table
            ),
            params![new_name, source_idx],
        ) {
            Ok(count) => {
                bevy::log::debug!(
                    "rename_data_column (metadata-only): UPDATE affected {} row(s)",
                    count
                );
            }
            Err(e) => {
                bevy::log::error!(
                    "rename_data_column (metadata-only): UPDATE failed with error: {}",
                    e
                );
                return Err(e.into());
            }
        }

        return Ok(());
    }
    
    // Old column exists physically - proceed with full rename
    // CRITICAL: Before renaming the physical column, check metadata for conflicts
    let meta_table = format!("{}_Metadata", table_name);
    
    // Get the column_index of the column we're renaming (using old_name)
    let source_column_index: Option<i32> = conn
        .query_row(
            &format!(
                "SELECT column_index FROM \"{}\" WHERE column_name = ?",
                meta_table
            ),
            params![old_name],
            |row| row.get(0),
        )
        .optional()?;
    
    let source_idx = match source_column_index {
        Some(idx) => idx,
        None => {
            bevy::log::error!(
                "rename_data_column (physical): Column '{}' not found in metadata table '{}'",
                old_name, meta_table
            );
            return Err(super::super::error::DbError::Other(format!(
                "Column '{}' not found in metadata table '{}'",
                old_name, meta_table
            )));
        }
    };
    
    bevy::log::debug!(
        "rename_data_column (physical): source column '{}' has index {}",
        old_name, source_idx
    );
    
    // Check if ANY column with the new name exists at a DIFFERENT index in metadata
    let conflicting_row: Option<(i32, i32)> = conn
        .query_row(
            &format!(
                "SELECT column_index, deleted FROM \"{}\" WHERE column_name = ?",
                meta_table
            ),
            params![new_name],
            |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?)),
        )
        .optional()?;
    
    bevy::log::debug!(
        "rename_data_column (physical): conflicting row check for '{}' returned {:?}",
        new_name, conflicting_row
    );
    
    if let Some((existing_idx, is_deleted)) = conflicting_row {
        if existing_idx != source_idx {
            // There's a column with this name at a different index
            if is_deleted == 1 {
                // Delete the conflicting deleted metadata row to allow reuse of the name
                bevy::log::warn!(
                    "Found deleted column '{}' at index {} in '{}' - deleting its metadata row to avoid conflict (source index={})",
                    new_name, existing_idx, meta_table, source_idx
                );
                conn.execute(
                    &format!(
                        "DELETE FROM \"{}\" WHERE column_name = ? AND deleted = 1",
                        meta_table
                    ),
                    params![new_name],
                )?;
            } else {
                // Active column with same name at different index - this is an error
                return Err(super::super::error::DbError::Other(format!(
                    "Column '{}' already exists at index {} in table '{}' (not deleted)",
                    new_name, existing_idx, table_name
                )));
            }
        }
        // If existing_idx == source_idx, that means metadata already has this name (weird but ok)
    }
    
    // Now safe to rename the physical column
    bevy::log::debug!(
        "rename_data_column (physical): executing ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"",
        table_name, old_name, new_name
    );
    
    conn.execute(
        &format!(
            "ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"",
            table_name, old_name, new_name
        ),
        [],
    )?;
    
    // Update metadata to match
    bevy::log::debug!(
        "rename_data_column (physical): executing UPDATE \"{}\" SET column_name = '{}' WHERE column_index = {}",
        meta_table, new_name, source_idx
    );
    
    match conn.execute(
        &format!(
            "UPDATE \"{}\" SET column_name = ? WHERE column_index = ?",
            meta_table
        ),
        params![new_name, source_idx],
    ) {
        Ok(count) => {
            bevy::log::debug!(
                "rename_data_column (physical): UPDATE affected {} row(s)",
                count
            );
        }
        Err(e) => {
            bevy::log::error!(
                "rename_data_column (physical): UPDATE failed with error: {}",
                e
            );
            return Err(e.into());
        }
    }
    
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
    
    // CRITICAL: Check if ANY column (deleted or not) with the new name already exists at a DIFFERENT index
    // If it does, we need to permanently delete it to avoid UNIQUE constraint violation
    let conflicting_row: Option<(i32, i32)> = conn
        .query_row(
            &format!(
                "SELECT column_index, deleted FROM \"{}\" WHERE column_name = ?",
                meta_table
            ),
            params![new_name],
            |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?)),
        )
        .optional()?;
    
    bevy::log::debug!(
        "update_metadata_column_name: conflicting row check for '{}' returned {:?}",
        new_name, conflicting_row
    );
    
    if let Some((existing_idx, is_deleted)) = conflicting_row {
        if existing_idx != persisted_index as i32 {
            // There's a column with this name at a different index
            if is_deleted == 1 {
                // Delete the conflicting deleted metadata row to allow reuse of the name
                bevy::log::warn!(
                    "Found deleted column '{}' at index {} in '{}' - deleting its metadata row to avoid conflict (target index={})",
                    new_name, existing_idx, meta_table, persisted_index
                );
                conn.execute(
                    &format!(
                        "DELETE FROM \"{}\" WHERE column_name = ? AND deleted = 1",
                        meta_table
                    ),
                    params![new_name],
                )?;
            } else {
                // Active column with same name at different index - this is an error
                return Err(super::super::error::DbError::Other(format!(
                    "Column '{}' already exists at index {} in table '{}' (not deleted)",
                    new_name, existing_idx, table_name
                )));
            }
        }
        // If existing_idx == persisted_index, the UPDATE will just replace the name (no conflict)
    }
    
    // Now safe to update the column name
    bevy::log::debug!(
        "update_metadata_column_name: executing UPDATE \"{}\" SET column_name = '{}' WHERE column_index = {}",
        meta_table, new_name, persisted_index
    );
    
    match conn.execute(
        &format!(
            "UPDATE \"{}\" SET column_name = ? WHERE column_index = ?",
            meta_table
        ),
        params![new_name, persisted_index as i32],
    ) {
        Ok(count) => {
            bevy::log::debug!(
                "update_metadata_column_name: UPDATE affected {} row(s)",
                count
            );
            Ok(())
        }
        Err(e) => {
            bevy::log::error!(
                "update_metadata_column_name: UPDATE failed with error: {}",
                e
            );
            Err(e.into())
        }
    }
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
