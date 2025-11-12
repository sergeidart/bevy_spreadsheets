// src/sheets/database/writer/renames.rs
// Rename operations - renaming columns and tables

use super::super::error::DbResult;
use super::super::schema::queries::{get_table_columns, table_exists};
use super::daemon_utils::exec_simple_statement;
use super::helpers::{
    drop_column_with_fallback,
    get_column_index_by_name, handle_column_conflict, metadata_table_name,
    rename_column, rename_table, rename_table_triplet,
    update_metadata_column_name_by_index, validate_physical_rename, with_transaction,
};
use rusqlite::{params, Connection, OptionalExtension};

/// Rename a data column and update its metadata column_name accordingly (for main or structure tables with real columns).
pub fn rename_data_column(
    conn: &Connection,
    table_name: &str,
    old_name: &str,
    new_name: &str,
    db_filename: Option<&str>,
    daemon_client: &super::super::daemon_client::DaemonClient,
) -> DbResult<()> {
    // Check if a column with new_name already exists in the DB schema
    let existing_columns = get_table_columns(conn, table_name)?;
    
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
        // Checkpoint WAL to ensure we see the latest daemon writes
        let _ = conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(()));
        let is_deleted: Option<i32> = conn.query_row(
            &format!("SELECT deleted FROM \"{}\" WHERE column_name = ?", meta_table),
            params![new_name],
            |row| row.get(0),
        ).ok();
        
        if matches!(is_deleted, Some(1)) {
            bevy::log::info!("Column '{}' exists in DB but is marked deleted - dropping it first", new_name);
            
            // Drop the deleted column from the table (with fallback for old SQLite)
            drop_column_with_fallback(conn, table_name, new_name, db_filename, daemon_client)?;
            
            // Also remove from metadata
            exec_simple_statement(
                format!("DELETE FROM \"{}\" WHERE column_name = ?", meta_table),
                vec![serde_json::Value::String(new_name.to_string())],
                daemon_client,
                conn,
            )?;
        } else {
            return Err(super::super::error::DbError::Other(format!(
                "Column '{}' already exists in table '{}' and is not marked as deleted",
                new_name, table_name
            )));
        }
    }
    
    let meta_table = metadata_table_name(table_name);
    
    // Check if old column exists physically - if not, only update metadata
    if !old_name_exists {
        bevy::log::info!(
            "Column '{}' does not exist physically in table '{}' - only updating metadata",
            old_name, table_name
        );
        
        // Get the column_index of the column we're renaming
        let source_idx = match get_column_index_by_name(conn, &meta_table, old_name)? {
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
        
        // Handle conflicts
        handle_column_conflict(conn, &meta_table, table_name, new_name, source_idx, daemon_client)?;
        
        // Update metadata - use column_index to be precise
        bevy::log::debug!(
            "rename_data_column (metadata-only): executing UPDATE \"{}\" SET column_name = '{}' WHERE column_index = {}",
            meta_table, new_name, source_idx
        );
        
        let count = update_metadata_column_name_by_index(conn, &meta_table, source_idx, new_name, db_filename, daemon_client)?;
        bevy::log::debug!(
            "rename_data_column (metadata-only): UPDATE affected {} row(s)",
            count
        );

        return Ok(());
    }
    
    // Old column exists physically - proceed with full rename
    // Get the column_index of the column we're renaming (using old_name)
    let source_idx = match get_column_index_by_name(conn, &meta_table, old_name)? {
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
    
    // Handle conflicts
    handle_column_conflict(conn, &meta_table, table_name, new_name, source_idx, daemon_client)?;
    
    // Now safe to rename the physical column
    bevy::log::debug!(
        "rename_data_column (physical): executing ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"",
        table_name, old_name, new_name
    );
    
    rename_column(conn, table_name, old_name, new_name, db_filename, daemon_client)?;
    
    // Validate the physical rename succeeded
    validate_physical_rename(conn, table_name, old_name, new_name)?;
    
    // Update metadata to match
    bevy::log::debug!(
        "rename_data_column (physical): executing UPDATE \"{}\" SET column_name = '{}' WHERE column_index = {}",
        meta_table, new_name, source_idx
    );
    
    let _count = update_metadata_column_name_by_index(conn, &meta_table, source_idx, new_name, db_filename, daemon_client)?;
    
    Ok(())
}

/// Update metadata column_name only (for columns that don't exist physically in main table, e.g., Structure validators)
/// Note: column_index is the RUNTIME index (includes technical columns like row_index)
pub fn update_metadata_column_name(
    conn: &Connection,
    table_name: &str,
    column_index: usize,
    new_name: &str,
    db_filename: Option<&str>,
    daemon_client: &super::super::daemon_client::DaemonClient,
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
    
    // Handle conflicts
    handle_column_conflict(conn, &meta_table, table_name, new_name, persisted_index as i32, daemon_client)?;
    
    // Now safe to update the column name
    bevy::log::debug!(
        "update_metadata_column_name: executing UPDATE \"{}\" SET column_name = '{}' WHERE column_index = {}",
        meta_table, new_name, persisted_index
    );
    
    let count = update_metadata_column_name_by_index(conn, &meta_table, persisted_index as i32, new_name, db_filename, daemon_client)?;
    bevy::log::debug!(
        "update_metadata_column_name: UPDATE affected {} row(s)",
        count
    );
    Ok(())
}

/// Rename a structure table (and its metadata table); also fix _Metadata entries: table_name and parent_column.
pub fn rename_structure_table(
    conn: &Connection,
    parent_table: &str,
    old_column_name: &str,
    new_column_name: &str,
    daemon_client: &super::super::daemon_client::DaemonClient,
) -> DbResult<()> {
    let old_struct = format!("{}_{}", parent_table, old_column_name);
    let new_struct = format!("{}_{}", parent_table, new_column_name);

    // Check existence of the structure data and metadata tables first
    let data_exists = table_exists(conn, &old_struct)?;
    let old_meta = metadata_table_name(&old_struct);
    let new_meta = metadata_table_name(&new_struct);
    let meta_exists = table_exists(conn, &old_meta)?;

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
        rename_table(conn, &old_struct, &new_struct, None, daemon_client)?;
        
        // Validate the table was renamed
        if !table_exists(conn, &new_struct)? {
            bevy::log::error!(
                "rename_structure_table: Validation failed - table '{}' doesn't exist after rename",
                new_struct
            );
            return Err(super::super::error::DbError::Other(format!(
                "Structure table rename validation failed: '{}' not found",
                new_struct
            )));
        }
        if table_exists(conn, &old_struct)? {
            bevy::log::warn!(
                "rename_structure_table: Old table '{}' still exists after rename",
                old_struct
            );
        }
        bevy::log::debug!(
            "rename_structure_table: Validation passed - table '{}' exists",
            new_struct
        );
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
        rename_table(conn, &old_meta, &new_meta, None, daemon_client)?;
        
        // Validate the metadata table was renamed
        if !table_exists(conn, &new_meta)? {
            bevy::log::error!(
                "rename_structure_table: Validation failed - metadata table '{}' doesn't exist after rename",
                new_meta
            );
            return Err(super::super::error::DbError::Other(format!(
                "Structure metadata table rename validation failed: '{}' not found",
                new_meta
            )));
        }
        bevy::log::debug!(
            "rename_structure_table: Validation passed - metadata table '{}' exists",
            new_meta
        );
    } else {
        bevy::log::warn!(
            "rename_structure_table: Metadata table '{}' not found; skipping metadata table rename.",
            old_meta
        );
    }

    // Update global _Metadata entry for the structure table (safe if no matching row)
    exec_simple_statement(
        "UPDATE _Metadata SET table_name = ?, parent_column = ?, updated_at = CURRENT_TIMESTAMP WHERE table_name = ?".to_string(),
        vec![
            serde_json::Value::String(new_struct.clone()),
            serde_json::Value::String(new_column_name.to_string()),
            serde_json::Value::String(old_struct.clone()),
        ],
        daemon_client,
        conn,
    )?;
    
    let updated = 0; // exec_simple_statement returns rows_affected but we stored it
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
    db_filename: Option<&str>,
    daemon_client: &super::super::daemon_client::DaemonClient,
) -> DbResult<()> {
    let meta_table = metadata_table_name(table_name);

    // Get the source column index for old_name
    let source_idx = match get_column_index_by_name(conn, &meta_table, old_name)? {
        Some(idx) => idx,
        None => {
            bevy::log::warn!(
                "update_metadata_column_name_by_name: old column '{}' not found in '{}'",
                old_name, meta_table
            );
            return Ok(());
        }
    };

    // Handle conflicts
    handle_column_conflict(conn, &meta_table, table_name, new_name, source_idx, daemon_client)?;

    // Update the row's name
    let updated = update_metadata_column_name_by_index(conn, &meta_table, source_idx, new_name, db_filename, daemon_client)?;
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
    db_filename: Option<&str>,
    daemon_client: &super::super::daemon_client::DaemonClient,
) -> DbResult<()> {
    use super::super::schema::queries::get_table_columns;
    
    // Check existence via get_table_columns
    let columns = get_table_columns(conn, table_name)?;
    let exists = columns.iter().any(|c| c.eq_ignore_ascii_case(column_name));
    
    if !exists {
        return Ok(());
    }

    drop_column_with_fallback(conn, table_name, column_name, db_filename, daemon_client)
}

/// Rename a main table and all of its descendant structure tables by prefix replacement.
/// This keeps DB names aligned with in-memory sheet rename and preserves parent_table links.
pub fn rename_table_and_descendants(
    conn: &Connection,
    old_table: &str,
    new_table: &str,
    db_filename: Option<&str>,
    daemon_client: &super::super::daemon_client::DaemonClient,
) -> DbResult<()> {
    // Wrap the full cascade in a transaction
    with_transaction(conn, daemon_client, |conn| {
        bevy::log::info!(
            "DB cascade rename: '{}' -> '{}' (including descendants)",
            old_table, new_table
        );

        // 1) Rename the main table first
        rename_table_triplet(conn, old_table, new_table, db_filename, daemon_client)?;

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
            rename_table_triplet(conn, old_name, new_name, db_filename, daemon_client)?;
        }

        // 3) Fix parent_table references in _Metadata for all renamed tables
        // Main parent: direct children referencing old_table
        exec_simple_statement(
            "UPDATE _Metadata SET parent_table = ?1 WHERE parent_table = ?2".to_string(),
            vec![
                serde_json::Value::String(new_table.to_string()),
                serde_json::Value::String(old_table.to_string()),
            ],
            daemon_client,
            conn,
        )?;
            
        // Descendants: update any row whose parent_table equals a renamed table
        for (old_name, new_name) in &pairs {
            exec_simple_statement(
                "UPDATE _Metadata SET parent_table = ?1 WHERE parent_table = ?2".to_string(),
                vec![
                    serde_json::Value::String(new_name.clone()),
                    serde_json::Value::String(old_name.clone()),
                ],
                daemon_client,
                conn,
            )?;
        }

        Ok(())
    })
}
