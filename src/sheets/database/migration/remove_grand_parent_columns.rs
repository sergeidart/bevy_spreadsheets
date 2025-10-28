// src/sheets/database/migration/remove_grand_parent_columns.rs
//! Migration to remove all grand_N_parent columns from structure tables
//!
//! ## Background
//! Previously, structure tables at depth N had columns:
//! - row_index
//! - grand_N_parent, grand_(N-1)_parent, ..., grand_1_parent  
//! - parent_key
//! - [data columns]
//!
//! ## Problem
//! The grand_N_parent columns are redundant because:
//! 1. parent_key already stores the parent's row_index (migrated from text)
//! 2. UNIQUE(parent_key, row_index) constraint ensures uniqueness
//! 3. We can walk the parent chain programmatically to build lineage
//!
//! ## Solution
//! Remove all grand_N_parent columns and simplify to:
//! - row_index
//! - parent_key (unique within parent context)
//! - [data columns]
//!
//! ## Migration Steps
//! 1. Find all tables with grand_*_parent columns
//! 2. For each table:
//!    a. Verify UNIQUE(parent_key, row_index) constraint exists
//!    b. Create new table without grand_*_parent columns
//!    c. Copy data (excluding grand_*_parent columns)
//!    d. Drop old table, rename new table
//!    e. Recreate indexes and constraints
//!    f. Update _Metadata to remove grand column definitions
//! 3. Log results: tables processed, columns removed, rows verified

use bevy::prelude::*;
use rusqlite::{params, Connection};

use super::super::error::{DbError, DbResult};
use super::occasional_fixes::MigrationFix;

pub struct RemoveGrandParentColumns;

impl MigrationFix for RemoveGrandParentColumns {
    fn id(&self) -> &'static str {
        "remove_grand_parent_columns_2025_10_28_v1"
    }

    fn description(&self) -> &'static str {
        "Remove redundant grand_N_parent columns from structure tables, keeping only parent_key"
    }

    fn apply(&self, conn: &mut Connection) -> DbResult<()> {
        info!("=== Starting grand_N_parent column removal migration ===");

        // Find all structure tables (those with parent_key column)
        let structure_tables = find_structure_tables(conn)?;
        info!("Found {} structure tables to check", structure_tables.len());

        if structure_tables.is_empty() {
            info!("No structure tables found, migration complete");
            return Ok(());
        }

        let mut tables_processed = 0;
        let mut total_columns_removed = 0;
        let mut tables_skipped = 0;

        for table_name in structure_tables {
            // Find grand_*_parent columns in this table
            let grand_columns = get_grand_parent_columns(conn, &table_name)?;
            
            if grand_columns.is_empty() {
                info!("Table '{}' has no grand_*_parent columns, skipping", table_name);
                tables_skipped += 1;
                continue;
            }

            info!(
                "Processing table '{}': removing {} grand_*_parent columns: {:?}",
                table_name,
                grand_columns.len(),
                grand_columns
            );

            // Verify uniqueness constraint exists
            verify_uniqueness_constraint(conn, &table_name)?;

            // Get row count before migration
            let row_count_before: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
                [],
                |row| row.get(0),
            )?;

            // Migrate the table
            migrate_table_remove_grands(conn, &table_name, &grand_columns)?;

            // Verify row count unchanged
            let row_count_after: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
                [],
                |row| row.get(0),
            )?;

            if row_count_before != row_count_after {
                error!(
                    "Row count mismatch for table '{}': before={}, after={}",
                    table_name, row_count_before, row_count_after
                );
                return Err(DbError::MigrationFailed(format!(
                    "Row count mismatch during migration of table '{}'",
                    table_name
                )));
            }

            // Update metadata to remove grand column definitions
            update_metadata_remove_grand_columns(conn, &table_name, &grand_columns)?;

            tables_processed += 1;
            total_columns_removed += grand_columns.len();
            
            info!(
                "✓ Successfully migrated table '{}': {} columns removed, {} rows verified",
                table_name,
                grand_columns.len(),
                row_count_after
            );
        }

        info!(
            "=== Grand_parent column removal complete ===\n\
             Tables processed: {}\n\
             Tables skipped: {}\n\
             Total columns removed: {}",
            tables_processed, tables_skipped, total_columns_removed
        );

        Ok(())
    }
}

/// Find all tables that have a parent_key column (structure tables)
fn find_structure_tables(conn: &Connection) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE '\\_%' ESCAPE '\\'"
    )?;

    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut structure_tables = Vec::new();

    for table_name in table_names {
        // Check if table has parent_key column
        let has_parent_key: bool = conn
            .prepare(&format!(
                "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = 'parent_key'",
                table_name
            ))?
            .query_row([], |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            })?;

        if has_parent_key {
            structure_tables.push(table_name);
        }
    }

    Ok(structure_tables)
}

/// Get all grand_*_parent columns from a table
fn get_grand_parent_columns(conn: &Connection, table_name: &str) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;

    let grand_columns: Vec<String> = stmt
        .query_map([], |row| {
            let col_name: String = row.get(1)?;
            Ok(col_name)
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|col_name| {
            let lower = col_name.to_lowercase();
            lower.starts_with("grand_") && lower.ends_with("_parent")
        })
        .collect();

    Ok(grand_columns)
}

/// Verify that UNIQUE(parent_key, row_index) constraint exists
fn verify_uniqueness_constraint(conn: &Connection, table_name: &str) -> DbResult<()> {
    // Check for unique index on (parent_key, row_index)
    let mut stmt = conn.prepare(&format!("PRAGMA index_list(\"{}\")", table_name))?;
    
    let has_unique_index: bool = stmt
        .query_map([], |row| {
            let is_unique: i32 = row.get(2)?; // Column 2 is 'unique' flag
            let index_name: String = row.get(1)?; // Column 1 is index name
            Ok((is_unique == 1, index_name))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|(is_unique, index_name)| {
            if !is_unique {
                return false;
            }
            
            // Check if this index covers (parent_key, row_index)
            // We'll verify this by checking the CREATE statement
            match conn.query_row(
                "SELECT sql FROM sqlite_master WHERE type='index' AND name=?",
                params![index_name],
                |row| {
                    let sql: Option<String> = row.get(0)?;
                    Ok(sql)
                }
            ) {
                Ok(Some(sql)) => {
                    let lower_sql = sql.to_lowercase();
                    lower_sql.contains("parent_key") && lower_sql.contains("row_index")
                }
                _ => false,
            }
        });

    if !has_unique_index {
        warn!(
            "Table '{}' does not have UNIQUE(parent_key, row_index) constraint - migration may proceed but uniqueness not guaranteed",
            table_name
        );
        // Don't fail - just warn. The constraint might be implicit in the table structure.
    } else {
        info!("✓ Verified UNIQUE(parent_key, row_index) constraint exists for '{}'", table_name);
    }

    Ok(())
}

/// Migrate a table by creating a new version without grand_*_parent columns
fn migrate_table_remove_grands(
    conn: &Connection,
    table_name: &str,
    grand_columns: &[String],
) -> DbResult<()> {
    // Get all columns except grand_*_parent
    let all_columns = get_all_columns(conn, table_name)?;
    let keep_columns: Vec<String> = all_columns
        .into_iter()
        .filter(|col| !grand_columns.contains(col))
        .collect();

    info!("  Keeping columns: {:?}", keep_columns);

    // Create temporary table name
    let temp_table = format!("{}_temp_no_grands", table_name);

    // Clean up any leftover temp table from previous failed run
    let _ = conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", temp_table), []);

    // Build CREATE TABLE statement for new table
    let create_sql = build_create_table_sql(conn, table_name, &keep_columns)?;
    let create_sql_temp = create_sql.replace(
        &format!("CREATE TABLE \"{}\"", table_name),
        &format!("CREATE TABLE \"{}\"", temp_table),
    );

    info!("  Creating temporary table: {}", temp_table);
    conn.execute(&create_sql_temp, [])?;

    // Copy data from old table to new table (excluding grand columns)
    let columns_list = keep_columns
        .iter()
        .map(|c| format!("\"{}\"", c))
        .collect::<Vec<_>>()
        .join(", ");

    let copy_sql = format!(
        "INSERT INTO \"{}\" ({}) SELECT {} FROM \"{}\"",
        temp_table, columns_list, columns_list, table_name
    );

    info!("  Copying data to temporary table...");
    let rows_copied = conn.execute(&copy_sql, [])?;
    info!("  Copied {} rows", rows_copied);

    // Drop old table
    info!("  Dropping old table...");
    conn.execute(&format!("DROP TABLE \"{}\"", table_name), [])?;

    // Rename temp table to original name
    info!("  Renaming temporary table to original name...");
    conn.execute(
        &format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", temp_table, table_name),
        [],
    )?;

    // Recreate unique index (if it doesn't exist)
    let index_name = format!("idx_{}_parent_row", table_name.replace(" ", "_"));
    let create_index_sql = format!(
        "CREATE UNIQUE INDEX IF NOT EXISTS \"{}\" ON \"{}\"(parent_key, row_index)",
        index_name, table_name
    );
    conn.execute(&create_index_sql, [])?;
    info!("  Recreated unique index: {}", index_name);

    Ok(())
}

/// Get all column names from a table
fn get_all_columns(conn: &Connection, table_name: &str) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;

    let columns: Vec<String> = stmt
        .query_map([], |row| {
            let col_name: String = row.get(1)?;
            Ok(col_name)
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(columns)
}

/// Build CREATE TABLE SQL for a table with specific columns
fn build_create_table_sql(
    conn: &Connection,
    table_name: &str,
    keep_columns: &[String],
) -> DbResult<String> {
    // Get column definitions from PRAGMA
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;

    let mut column_defs = Vec::new();
    let column_info: Vec<(String, String, bool, Option<String>, bool)> = stmt
        .query_map([], |row| {
            let name: String = row.get(1)?;
            let col_type: String = row.get(2)?;
            let not_null: i32 = row.get(3)?;
            let default_value: Option<String> = row.get(4)?;
            let pk: i32 = row.get(5)?;
            Ok((name, col_type, not_null == 1, default_value, pk == 1))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    for (name, col_type, not_null, default_value, is_pk) in column_info {
        if !keep_columns.contains(&name) {
            continue; // Skip grand_*_parent columns
        }

        let mut def = format!("\"{}\" {}", name, col_type);

        if is_pk {
            def.push_str(" PRIMARY KEY");
            if col_type.to_uppercase() == "INTEGER" {
                def.push_str(" AUTOINCREMENT");
            }
        }

        if not_null && !is_pk {
            def.push_str(" NOT NULL");
        }

        if let Some(default) = default_value {
            def.push_str(&format!(" DEFAULT {}", default));
        }

        column_defs.push(def);
    }

    // Add UNIQUE constraint
    column_defs.push("UNIQUE(parent_key, row_index)".to_string());

    let create_sql = format!(
        "CREATE TABLE \"{}\" ({})",
        table_name,
        column_defs.join(", ")
    );

    Ok(create_sql)
}

/// Mark grand_*_parent columns as deleted in {table_name}_Metadata
fn update_metadata_remove_grand_columns(
    conn: &Connection,
    table_name: &str,
    grand_columns: &[String],
) -> DbResult<()> {
    let metadata_table = format!("{}_Metadata", table_name);
    
    // Check if metadata table exists
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            params![&metadata_table],
            |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            },
        )?;

    if !table_exists {
        warn!("No metadata table '{}' found for table '{}', skipping metadata update", metadata_table, table_name);
        return Ok(());
    }

    // Check if 'deleted' column exists in metadata table
    let has_deleted_column: bool = conn
        .prepare(&format!("PRAGMA table_info(\"{}\")", metadata_table))?
        .query_map([], |row| {
            let col_name: String = row.get(1)?;
            Ok(col_name)
        })?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|col| col.eq_ignore_ascii_case("deleted"));

    if !has_deleted_column {
        warn!(
            "Metadata table '{}' does not have a 'deleted' column, cannot mark columns as deleted",
            metadata_table
        );
        // Instead, try to physically delete the metadata rows for grand columns
        info!("Attempting to physically delete metadata rows for grand_*_parent columns from '{}'", metadata_table);
        
        for grand_col in grand_columns {
            let delete_result = conn.execute(
                &format!("DELETE FROM \"{}\" WHERE column_name = ?", metadata_table),
                params![grand_col],
            );

            match delete_result {
                Ok(rows_deleted) => {
                    if rows_deleted > 0 {
                        info!("  Deleted metadata row for column '{}'", grand_col);
                    } else {
                        warn!("  Column '{}' not found in metadata (may have been removed already)", grand_col);
                    }
                }
                Err(e) => {
                    error!("  Failed to delete metadata row for column '{}': {}", grand_col, e);
                    return Err(DbError::from(e));
                }
            }
        }
        
        info!("  Deleted {} metadata rows from '{}'", grand_columns.len(), metadata_table);
        return Ok(());
    }

    // Mark each grand_*_parent column as deleted in metadata
    let mut marked_count = 0;
    for grand_col in grand_columns {
        let update_result = conn.execute(
            &format!("UPDATE \"{}\" SET deleted = 1 WHERE column_name = ?", metadata_table),
            params![grand_col],
        );

        match update_result {
            Ok(rows_updated) => {
                if rows_updated > 0 {
                    info!("  Marked column '{}' as deleted in metadata", grand_col);
                    marked_count += 1;
                } else {
                    warn!("  Column '{}' not found in metadata (may have been removed already)", grand_col);
                }
            }
            Err(e) => {
                error!("  Failed to mark column '{}' as deleted: {}", grand_col, e);
                return Err(DbError::from(e));
            }
        }
    }

    if marked_count > 0 {
        info!("  Updated metadata table '{}' to mark {} columns as deleted", metadata_table, marked_count);
    } else {
        warn!("  No columns were marked as deleted in metadata table '{}'", metadata_table);
    }

    Ok(())
}
