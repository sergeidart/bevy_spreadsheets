// src/sheets/database/writer/renames.rs
// Rename operations - renaming columns and tables

use super::super::error::DbResult;
use super::helpers::metadata_table_name;
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
        let meta_table = metadata_table_name(table_name);
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
        let meta_table = metadata_table_name(table_name);
        
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
    let meta_table = metadata_table_name(table_name);
    
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
    
    let meta_table = metadata_table_name(table_name);
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

    // Check existence of the structure data and metadata tables first
    let data_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![&old_struct],
            |row| row.get::<_, i32>(0),
        )
        .optional()?
        .is_some();

    let old_meta = metadata_table_name(&old_struct);
    let new_meta = metadata_table_name(&new_struct);
    let meta_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![&old_meta],
            |row| row.get::<_, i32>(0),
        )
        .optional()?
        .is_some();

    if !data_exists && !meta_exists {
        bevy::log::info!(
            "rename_structure_table: No physical tables found for '{}' (old structure for parent '{}'). Treating as no-op.",
            old_struct,
            parent_table
        );
        return Ok(());
    }

    if data_exists {
        bevy::log::info!(
            "rename_structure_table: Renaming data table '{}' -> '{}'",
            old_struct, new_struct
        );
        conn.execute(
            &format!(
                "ALTER TABLE \"{}\" RENAME TO \"{}\"",
                old_struct, new_struct
            ),
            [],
        )?;
    } else {
        bevy::log::warn!(
            "rename_structure_table: Data table '{}' not found; skipping data table rename.",
            old_struct
        );
    }

    if meta_exists {
        bevy::log::info!(
            "rename_structure_table: Renaming metadata table '{}' -> '{}'",
            old_meta, new_meta
        );
        conn.execute(
            &format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", old_meta, new_meta),
            [],
        )?;
    } else {
        bevy::log::warn!(
            "rename_structure_table: Metadata table '{}' not found; skipping metadata table rename.",
            old_meta
        );
    }

    // Update global _Metadata entry for the structure table (safe if no matching row)
    let updated = conn.execute(
        "UPDATE _Metadata SET table_name = ?, parent_column = ?, updated_at = CURRENT_TIMESTAMP WHERE table_name = ?",
        params![&new_struct, new_column_name, &old_struct],
    )?;
    if updated == 0 {
        bevy::log::info!(
            "rename_structure_table: No _Metadata row for '{}' to update; structure may be newly created without registration yet.",
            old_struct
        );
    }

    Ok(())
}

/// Update parent table's metadata column_name by matching the old column name.
/// Performs conflict checks similar to index-based update and supports deleted-row cleanup.
pub fn update_metadata_column_name_by_name(
    conn: &Connection,
    table_name: &str,
    old_name: &str,
    new_name: &str,
) -> DbResult<()> {
    let meta_table = metadata_table_name(table_name);

    // Get the source column index for old_name
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
            bevy::log::warn!(
                "update_metadata_column_name_by_name: old column '{}' not found in '{}'",
                old_name, meta_table
            );
            return Ok(());
        }
    };

    // Check conflict for new_name at a different index
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

    if let Some((existing_idx, is_deleted)) = conflicting_row {
        if existing_idx != source_idx {
            if is_deleted == 1 {
                bevy::log::warn!(
                    "update_metadata_column_name_by_name: deleting conflicting deleted row '{}' at index {} in '{}'",
                    new_name, existing_idx, meta_table
                );
                conn.execute(
                    &format!(
                        "DELETE FROM \"{}\" WHERE column_name = ? AND deleted = 1",
                        meta_table
                    ),
                    params![new_name],
                )?;
            } else {
                return Err(super::super::error::DbError::Other(format!(
                    "Column '{}' already exists at a different index in table '{}'",
                    new_name, table_name
                )));
            }
        }
    }

    // Update the row's name
    let updated = conn.execute(
        &format!(
            "UPDATE \"{}\" SET column_name = ? WHERE column_index = ?",
            meta_table
        ),
        params![new_name, source_idx],
    )?;
    bevy::log::info!(
        "update_metadata_column_name_by_name: updated {} row(s) for '{}.{}' -> '{}.{}'",
        updated, table_name, old_name, table_name, new_name
    );
    Ok(())
}

/// Best-effort: drop a physical column from a main table if it exists.
/// Used to clean up leftover physical columns when converting to Structure or after renames.
pub fn drop_physical_column_if_exists(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
) -> DbResult<()> {
    // Check existence via PRAGMA table_info
    let mut exists = false;
    if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name)) {
        let rows = stmt.query_map([], |row| row.get::<_, String>(1));
        if let Ok(iter) = rows {
            for r in iter.flatten() {
                if r.eq_ignore_ascii_case(column_name) {
                    exists = true;
                    break;
                }
            }
        }
    }
    if !exists {
        return Ok(());
    }

    bevy::log::info!(
        "drop_physical_column_if_exists: attempting to drop '{}.{}'",
        table_name, column_name
    );

    // Try to wipe values first (safe even if DROP fails)
    let _ = conn.execute(
        &format!("UPDATE \"{}\" SET \"{}\" = NULL", table_name, column_name),
        [],
    );

    // Attempt DROP COLUMN (SQLite 3.35.0+)
    match conn.execute(
        &format!(
            "ALTER TABLE \"{}\" DROP COLUMN \"{}\"",
            table_name, column_name
        ),
        [],
    ) {
        Ok(_) => {
            bevy::log::info!(
                "drop_physical_column_if_exists: dropped column '{}.{}'",
                table_name, column_name
            );
        }
        Err(e) => {
            bevy::log::warn!(
                "drop_physical_column_if_exists: failed to drop column '{}.{}': {} (column may persist)",
                table_name, column_name, e
            );
        }
    }
    Ok(())
}

/// Rename a main table and all of its descendant structure tables by prefix replacement.
/// This keeps DB names aligned with in-memory sheet rename and preserves parent_table links.
pub fn rename_table_and_descendants(
    conn: &Connection,
    old_table: &str,
    new_table: &str,
) -> DbResult<()> {
    // Wrap the full cascade in a transaction
    conn.execute("BEGIN IMMEDIATE", [])?;
    let result = (|| -> DbResult<()> {
        // Helper to rename a data table + its metadata and AI groups tables (if present)
        fn rename_table_triplet(
            conn: &Connection,
            old_name: &str,
            new_name: &str,
        ) -> DbResult<()> {
            // If target metadata (or groups) already exists without a real data table, clean it up first
            let new_data_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [new_name],
                    |row| row.get::<_, i32>(0),
                )
                .optional()?
                .is_some();

            let new_meta = metadata_table_name(new_name);
            let new_meta_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [&new_meta],
                    |row| row.get::<_, i32>(0),
                )
                .optional()?
                .is_some();
            if new_meta_exists && !new_data_exists {
                bevy::log::warn!(
                    "Found orphan metadata table '{}' without data table '{}'; dropping before rename.",
                    new_meta,
                    new_name
                );
                conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", new_meta), [])?;
            }

            let new_groups = format!("{}_AIGroups", new_name);
            let new_groups_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [&new_groups],
                    |row| row.get::<_, i32>(0),
                )
                .optional()?
                .is_some();
            if new_groups_exists && !new_data_exists {
                bevy::log::warn!(
                    "Found orphan AI groups table '{}' without data table '{}'; dropping before rename.",
                    new_groups,
                    new_name
                );
                conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", new_groups), [])?;
            }

            // Data table
            let data_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [old_name],
                    |row| row.get::<_, i32>(0),
                )
                .optional()?
                .is_some();

            if data_exists {
                conn.execute(
                    &format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", old_name, new_name),
                    [],
                )?;
            } else {
                bevy::log::warn!(
                    "rename_table_and_descendants: Data table '{}' not found; skipping data rename.",
                    old_name
                );
            }

            // Metadata table
            let old_meta = metadata_table_name(old_name);
            let new_meta = metadata_table_name(new_name);
            let meta_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [&old_meta],
                    |row| row.get::<_, i32>(0),
                )
                .optional()?
                .is_some();
            if meta_exists {
                conn.execute(
                    &format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", old_meta, new_meta),
                    [],
                )?;
            } else {
                bevy::log::warn!(
                    "rename_table_and_descendants: Metadata table '{}' not found; skipping metadata rename.",
                    old_meta
                );
            }

            // AI Groups table (optional)
            let old_groups = format!("{}_AIGroups", old_name);
            let new_groups = format!("{}_AIGroups", new_name);
            let groups_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [&old_groups],
                    |row| row.get::<_, i32>(0),
                )
                .optional()?
                .is_some();
            if groups_exists {
                conn.execute(
                    &format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", old_groups, new_groups),
                    [],
                )?;
            }

            // Update global metadata table row for the renamed table, if present
            // Remove any orphaned row for the target name first to avoid UNIQUE constraint violations
            conn.execute(
                "DELETE FROM _Metadata WHERE table_name = ?",
                params![new_name],
            )?;
            conn.execute(
                "UPDATE _Metadata SET table_name = ?, updated_at = CURRENT_TIMESTAMP WHERE table_name = ?",
                params![new_name, old_name],
            )?;

            Ok(())
        }

        bevy::log::info!(
            "DB cascade rename: '{}' -> '{}' (including descendants)",
            old_table, new_table
        );

        // 1) Rename the main table first
        rename_table_triplet(conn, old_table, new_table)?;

        // 2) Collect descendant structure tables using prefix match on _Metadata
        let prefix = format!("{}_", old_table);
        let like = format!("{}%", prefix);
        let mut stmt = conn.prepare(
            "SELECT table_name FROM _Metadata WHERE table_type = 'structure' AND table_name LIKE ?1",
        )?;
        let mut pairs: Vec<(String, String)> = stmt
            .query_map([like.as_str()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|old_name| {
                let suffix = &old_name[prefix.len()..];
                let new_name = format!("{}_{}", new_table, suffix);
                (old_name, new_name)
            })
            .collect();

        // Rename deepest-first to reduce transient name conflicts
        pairs.sort_by_key(|(o, _)| std::cmp::Reverse(o.len()));

        for (old_name, new_name) in &pairs {
            rename_table_triplet(conn, old_name, new_name)?;
        }

        // 3) Fix parent_table references in _Metadata for all renamed tables
        // Main parent: direct children referencing old_table
        conn.execute(
            "UPDATE _Metadata SET parent_table = ?1 WHERE parent_table = ?2",
            params![new_table, old_table],
        )?;
        // Descendants: update any row whose parent_table equals a renamed table
        for (old_name, new_name) in &pairs {
            conn.execute(
                "UPDATE _Metadata SET parent_table = ?1 WHERE parent_table = ?2",
                params![new_name, old_name],
            )?;
        }

        Ok(())
    })();

    match result {
        Ok(_) => {
            conn.execute("COMMIT", [])?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", []);
            Err(e)
        }
    }
}
