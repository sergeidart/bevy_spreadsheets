// src/sheets/database/migration/parent_key_to_row_index.rs

use bevy::prelude::*;
use rusqlite::{Connection, OptionalExtension};

use super::occasional_fixes::MigrationFix;
use super::super::error::DbResult;

/// Migration to convert parent_key values from text-based to row_index-based
///
/// Issue: Currently parent_key columns store text values (e.g., "Mass Effect 3")
/// which break when the parent row is renamed. This migration converts all
/// parent_key values to numeric row_index values for stable references.
///
/// Benefits:
/// - Renaming parents won't break child connections
/// - Faster numeric index lookups vs text matching
/// - Simpler cascade logic with numeric keys
///
/// Process:
/// 1. Find all tables with parent_key column (structure tables)
/// 2. For each table, batch process rows (1,000 at a time)
/// 3. For text-based parent_key values, resolve to parent's row_index
/// 4. Uses full ancestor chain for matching (not just immediate parent)
/// 5. Logs: migrated count, skipped count, broken references
pub struct MigrateParentKeyToRowIndex;

impl MigrationFix for MigrateParentKeyToRowIndex {
    fn id(&self) -> &str {
        "migrate_parent_key_to_row_index_2025_10_27_v4"
    }

    fn description(&self) -> &str {
        "Convert parent_key from text values to numeric row_index for stable references"
    }

    fn apply(&self, conn: &mut Connection) -> DbResult<()> {
        info!("=== Starting parent_key to row_index migration ===");

        // Get all table names from global metadata
        let tables: Vec<String> = conn
            .prepare("SELECT table_name FROM _Metadata ORDER BY display_order")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        info!("Migration: Found {} tables to check", tables.len());

        let mut total_migrated = 0;
        let mut total_skipped = 0;
        let mut total_broken = 0;
        let mut tables_processed = 0;

        for (table_idx, table_name) in tables.iter().enumerate() {
            info!("Migration: Checking table {}/{}: '{}'", table_idx + 1, tables.len(), table_name);
            // Check if table has parent_key column (indicates structure table)
            let has_parent_key: bool = conn
                .prepare(&format!(
                    "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = 'parent_key'",
                    table_name
                ))?
                .query_row([], |row| {
                    let count: i32 = row.get(0)?;
                    Ok(count > 0)
                })?;

            if !has_parent_key {
                continue; // Not a structure table, skip
            }

            // Check if table also has row_index column
            let has_row_index: bool = conn
                .prepare(&format!(
                    "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = 'row_index'",
                    table_name
                ))?
                .query_row([], |row| {
                    let count: i32 = row.get(0)?;
                    Ok(count > 0)
                })?;

            if !has_row_index {
                warn!("Table '{}' has parent_key but no row_index column, skipping", table_name);
                continue;
            }

            info!("Processing structure table: '{}'", table_name);

            // Determine parent table name (format: ParentTable_ColumnName)
            let parent_table = match table_name.rsplit_once('_') {
                Some((parent, _)) => parent,
                None => {
                    warn!("Cannot determine parent table from structure table name: {}", table_name);
                    continue;
                }
            };

            // Check if parent table exists
            let parent_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [parent_table],
                    |row| {
                        let count: i32 = row.get(0)?;
                        Ok(count > 0)
                    },
                )?;

            if !parent_exists {
                warn!("Parent table '{}' not found for structure table '{}', skipping",
                      parent_table, table_name);
                continue;
            }

            // Get total rows in structure table
            let total_rows: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
                [],
                |row| row.get(0),
            )?;

            if total_rows == 0 {
                info!("  ↳ Table '{}' is empty, skipping", table_name);
                continue;
            }

            info!("  ↳ Table '{}' has {} rows, processing...", table_name, total_rows);

            // Get all ancestor key columns (grand_*_parent columns)
            let ancestor_columns = get_ancestor_columns(conn, table_name)?;

            // Process in batches of 1,000 rows
            let batch_size = 1000;
            let num_batches = ((total_rows as f64) / (batch_size as f64)).ceil() as i64;

            info!("  ↳ Processing in {} batches ({} rows per batch)", num_batches, batch_size);

            let (table_migrated, table_skipped, table_broken) = process_table_batches(
                conn,
                table_name,
                parent_table,
                &ancestor_columns,
                batch_size,
                num_batches,
                total_rows,
            )?;

            total_migrated += table_migrated;
            total_skipped += table_skipped;
            total_broken += table_broken;
            tables_processed += 1;

            info!(
                "Completed '{}': {} migrated, {} skipped, {} broken",
                table_name, table_migrated, table_skipped, table_broken
            );
        }

        info!(
            "Parent_key migration complete: {} tables processed, {} rows migrated, {} skipped, {} broken references",
            tables_processed, total_migrated, total_skipped, total_broken
        );

        Ok(())
    }
}

/// Get all ancestor key columns (grand_*_parent) from a table
fn get_ancestor_columns(conn: &Connection, table_name: &str) -> DbResult<Vec<String>> {
    let mut columns: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?
        .query_map([], |row| {
            let name: String = row.get(1)?;
            Ok(name)
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|name| {
            let lower = name.to_lowercase();
            lower.starts_with("grand_") && lower.ends_with("_parent")
        })
        .collect();

    // Sort by numeric level if possible (grand_1, grand_2, ...)
    columns.sort_by(|a, b| {
        let na = a
            .strip_prefix("grand_")
            .and_then(|s| s.strip_suffix("_parent"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        let nb = b
            .strip_prefix("grand_")
            .and_then(|s| s.strip_suffix("_parent"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        na.cmp(&nb)
    });

    Ok(columns)
}

/// Process table in batches
fn process_table_batches(
    conn: &mut Connection,
    table_name: &str,
    parent_table: &str,
    ancestor_columns: &[String],
    batch_size: i64,
    num_batches: i64,
    total_rows: i64,
) -> DbResult<(usize, usize, usize)> {
    let mut total_migrated = 0;
    let mut total_skipped = 0;
    let mut total_broken = 0;

    for batch_idx in 0..num_batches {
        let offset = batch_idx * batch_size;

        // Fetch batch of rows
        let query = format!(
            "SELECT id, row_index, parent_key {} FROM \"{}\" ORDER BY id LIMIT {} OFFSET {}",
            if ancestor_columns.is_empty() {
                String::new()
            } else {
                format!(", {}", ancestor_columns.iter()
                    .map(|c| format!("\"{}\"", c))
                    .collect::<Vec<_>>()
                    .join(", "))
            },
            table_name,
            batch_size,
            offset
        );

        #[derive(Default)]
        struct RowUpdates {
            id: i64,
            parent_key_new: Option<String>,
            ancestor_updates: Vec<(String, String)>, // (column_name, new_value)
        }

        let mut rows_to_update: Vec<RowUpdates> = Vec::new();
        let mut broken_refs: Vec<(i64, String, Vec<String>)> = Vec::new(); // (row_index, parent_key, ancestors)

        let mut batch_migrated = 0;
        let mut batch_skipped = 0;

        // Fetch and process rows - ensure stmt is dropped before transaction
        {
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map([], |row| {
                let id: i64 = row.get(0)?;
                let row_index: i64 = row.get(1)?;
                let parent_key: String = row.get(2)?;

                // Get ancestor values
                let mut ancestors = Vec::new();
                for i in 0..ancestor_columns.len() {
                    let ancestor_val: String = row.get(3 + i)?;
                    ancestors.push(ancestor_val);
                }

                Ok((id, row_index, parent_key, ancestors))
            })?;

            for row_result in rows {
                let (id, row_index, parent_key, ancestors) = row_result?;

                let mut update = RowUpdates { id, ..Default::default() };

                // 1) Migrate parent_key text -> numeric
                if parent_key.parse::<i64>().is_ok() {
                    batch_skipped += 1; // parent_key already numeric
                } else {
                    match resolve_parent_row_index(conn, parent_table, &parent_key, &ancestors, ancestor_columns) {
                        Ok(Some(parent_row_idx)) => {
                            update.parent_key_new = Some(parent_row_idx.to_string());
                            batch_migrated += 1;
                        }
                        Ok(None) => {
                            // Parent not found, mark as broken (but we still attempt ancestor fixes below)
                            broken_refs.push((row_index, parent_key.clone(), ancestors.clone()));
                        }
                        Err(e) => {
                            warn!("Error resolving parent_key '{}' for row {}: {}", parent_key, row_index, e);
                            broken_refs.push((row_index, parent_key.clone(), ancestors.clone()));
                        }
                    }
                }

                // 2) Migrate each grand_*_parent text -> numeric
                for (i, anc_val) in ancestors.iter().enumerate() {
                    if anc_val.trim().is_empty() || anc_val.parse::<i64>().is_ok() { continue; }
                    if let Some(child_ancestor_col) = ancestor_columns.get(i) {
                        // Parse N from grand_N_parent; ancestor table is (N+1) levels up from current child table
                        let n_opt = child_ancestor_col
                            .strip_prefix("grand_")
                            .and_then(|s| s.strip_suffix("_parent"))
                            .and_then(|s| s.parse::<usize>().ok());
                        if let Some(n) = n_opt {
                            // Compute ancestor table name: start at child.parent_table and go up n levels more
                            let mut ancestor_table = parent_table.to_string();
                            let mut ok = true;
                            for _ in 0..n { // already up 1 to parent; n more to reach N+1
                                if let Some((up, _)) = ancestor_table.rsplit_once('_') { ancestor_table = up.to_string(); } else { ok = false; break; }
                            }
                            if ok {
                                // Find first non-technical data column in ancestor table
                                let ancestor_cols: Vec<(String, String)> = conn
                                    .prepare(&format!("PRAGMA table_info(\"{}\")", ancestor_table))?
                                    .query_map([], |row| {
                                        let name: String = row.get(1)?;
                                        let col_type: String = row.get(2)?;
                                        Ok((name, col_type))
                                    })?
                                    .collect::<Result<Vec<_>, _>>()?;
                                let anc_key_col = ancestor_cols.iter().find(|(name, _)| {
                                    let lower = name.to_lowercase();
                                    lower != "id" && lower != "row_index" && lower != "parent_key" && !lower.starts_with("grand_") && lower != "created_at" && lower != "updated_at"
                                }).map(|(n, _)| n.clone());
                                if let Some(akey) = anc_key_col {
                                    let idx_res: Result<i64, _> = conn.query_row(
                                        &format!("SELECT row_index FROM \"{}\" WHERE LOWER(\"{}\") = LOWER(?) LIMIT 1", ancestor_table, akey),
                                        [anc_val],
                                        |row| row.get(0),
                                    );
                                    if let Ok(v) = idx_res { update.ancestor_updates.push((child_ancestor_col.clone(), v.to_string())); }
                                }
                            }
                        }
                    }
                }

                if update.parent_key_new.is_some() || !update.ancestor_updates.is_empty() {
                    rows_to_update.push(update);
                }
            }
        } // stmt is dropped here

        // Perform batch update
        if !rows_to_update.is_empty() {
            let tx = conn.transaction()?;
            for upd in &rows_to_update {
                if let Some(pk_new) = &upd.parent_key_new {
                    tx.execute(
                        &format!("UPDATE \"{}\" SET parent_key = ?1 WHERE id = ?2", table_name),
                        rusqlite::params![pk_new, &upd.id],
                    )?;
                }
                for (col, val) in &upd.ancestor_updates {
                    tx.execute(
                        &format!("UPDATE \"{}\" SET \"{}\" = ?1 WHERE id = ?2", table_name, col),
                        rusqlite::params![val, &upd.id],
                    )?;
                }
            }
            tx.commit()?;
        }

        total_migrated += batch_migrated;
        total_skipped += batch_skipped;
        total_broken += broken_refs.len();

        // Log batch results
        if batch_migrated > 0 || !broken_refs.is_empty() {
            info!(
                "    Batch {}/{} (rows {}-{}): ✓ {} migrated, - {} skipped{}",
                batch_idx + 1,
                num_batches,
                offset + 1,
                (offset + batch_size).min(total_rows),
                batch_migrated,
                batch_skipped,
                if broken_refs.is_empty() {
                    String::new()
                } else {
                    format!(", ⚠ {} broken", broken_refs.len())
                }
            );

            // Log broken references (only first 5 per batch to avoid spam)
            for (i, (row_idx, parent_key, ancestors)) in broken_refs.iter().take(5).enumerate() {
                let ancestor_context = if ancestors.is_empty() {
                    String::new()
                } else {
                    format!(" (ancestors: [{}])", ancestors.join(", "))
                };
                warn!(
                    "      ⚠ Row {} (row_index={}): parent \"{}\" not found{}",
                    i + 1, row_idx, parent_key, ancestor_context
                );
            }
            if broken_refs.len() > 5 {
                warn!("      ... and {} more broken references in this batch", broken_refs.len() - 5);
            }
        }
    }

    Ok((total_migrated, total_skipped, total_broken))
}

/// Resolve text parent_key to parent's row_index using full ancestor chain
///
/// This builds a WHERE clause using ALL ancestor conditions to find the correct parent.
/// Example: For a row with ancestors ["Mass Effect 3", "Steam", "Buy"],
/// it searches the parent table for a row matching ALL three conditions.
fn resolve_parent_row_index(
    conn: &Connection,
    parent_table: &str,
    parent_key_text: &str,
    child_ancestors: &[String],
    child_ancestor_columns: &[String],
) -> DbResult<Option<i64>> {
    // Get parent table columns (for existence checks on technical columns)
    let parent_columns: Vec<(String, String)> = conn
        .prepare(&format!("PRAGMA table_info(\"{}\")", parent_table))?
        .query_map([], |row| {
            let name: String = row.get(1)?;
            let col_type: String = row.get(2)?;
            Ok((name, col_type))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // Primary display column must be derived from metadata order, not PRAGMA order
    let key_column = get_primary_display_column(conn, parent_table)?;
    let Some(key_column) = key_column else { return Ok(None) };

    // Match only that primary data column, case-insensitive
    let mut where_conditions = vec![format!("LOWER(\"{}\") = LOWER(?)", key_column)];
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(1 + child_ancestors.len());
    params.push(Box::new(parent_key_text.to_string()));

    // Add ancestor conditions by mapping child's grand_N_parent to parent's column:
    // - grand_1_parent (child) -> parent_key (parent)
    // - grand_N_parent (child) -> grand_{N-1}_parent (parent) for N >= 2
    for (i, child_ancestor_val) in child_ancestors.iter().enumerate() {
        if child_ancestor_val.trim().is_empty() {
            continue;
        }

        let Some(child_ancestor_col) = child_ancestor_columns.get(i) else { continue };
        // Parse N from grand_N_parent
        let n_opt = child_ancestor_col
            .strip_prefix("grand_")
            .and_then(|s| s.strip_suffix("_parent"))
            .and_then(|s| s.parse::<usize>().ok());

        if let Some(n) = n_opt {
            // Determine corresponding parent column name
            let parent_col_name = if n == 1 {
                "parent_key".to_string()
            } else {
                format!("grand_{}_parent", n - 1)
            };

            // Only add condition if parent actually has this column
            if parent_columns.iter().any(|(name, _)| name.eq_ignore_ascii_case(&parent_col_name)) {
                // Determine the numeric row_index to match against parent's column.
                // If child's ancestor value is already numeric, use it directly.
                let numeric_idx_str = if child_ancestor_val.parse::<i64>().is_ok() {
                    Some(child_ancestor_val.clone())
                } else {
                    // Resolve by looking up the ancestor table's first data column
                    // Compute ancestor table name by navigating up 'n' levels from parent_table
                    let mut ancestor_table = parent_table.to_string();
                    let mut ok = true;
                    for _ in 0..n {
                        if let Some((up, _)) = ancestor_table.rsplit_once('_') {
                            ancestor_table = up.to_string();
                        } else {
                            ok = false; // Can't navigate further up
                            break;
                        }
                    }

                    if ok {
                        // Resolve by metadata-defined primary column of the ancestor table
                        match resolve_text_row_index_by_meta(conn, &ancestor_table, child_ancestor_val)? {
                            Some(v) => Some(v.to_string()),
                            None => None,
                        }
                    } else {
                        None
                    }
                };

                if let Some(idx_str) = numeric_idx_str {
                    where_conditions.push(format!("\"{}\" = ?", parent_col_name));
                    params.push(Box::new(idx_str));
                } else {
                    // Could not resolve ancestor; enforce strict AND semantics by failing to match
                    return Ok(None);
                }
            }
        }
    }

    let where_clause = where_conditions.join(" AND ");
    let query = format!(
        "SELECT row_index FROM \"{}\" WHERE {} LIMIT 1",
        parent_table, where_clause
    );

    let result = conn.query_row(
        &query,
        rusqlite::params_from_iter(params.iter()),
        |row| row.get::<_, i64>(0),
    );

    match result {
        Ok(row_idx) => Ok(Some(row_idx)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Read the primary display/data column name from the table's metadata.
/// Returns the first non-deleted column_name ordered by column_index.
fn get_primary_display_column(conn: &Connection, table_name: &str) -> DbResult<Option<String>> {
    let meta_table = format!("{}_Metadata", table_name);
    // Ensure metadata table exists
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [meta_table.as_str()],
            |row| {
                let c: i32 = row.get(0)?;
                Ok(c > 0)
            },
        )
        .unwrap_or(false);
    if !exists {
        // Fallback to first non-technical via PRAGMA if metadata missing
        let cols: Vec<String> = conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let cand = cols.into_iter().find(|name| {
            let lower = name.to_lowercase();
            lower != "id"
                && lower != "row_index"
                && lower != "parent_key"
                && !lower.starts_with("grand_")
                && lower != "created_at"
                && lower != "updated_at"
        });
        return Ok(cand);
    }

    let sql = format!(
        "SELECT column_name FROM \"{}\" WHERE COALESCE(deleted,0)=0 ORDER BY column_index LIMIT 1",
        meta_table
    );
    let name_opt: Option<String> = conn.query_row(&sql, [], |row| row.get(0)).optional()?;
    Ok(name_opt)
}

/// Resolve a display text to row_index for the given table, using the metadata-defined
/// primary display column. Case-insensitive match.
fn resolve_text_row_index_by_meta(
    conn: &Connection,
    table_name: &str,
    display_text: &str,
) -> DbResult<Option<i64>> {
    let Some(primary_col) = get_primary_display_column(conn, table_name)? else { return Ok(None) };
    let query = format!(
        "SELECT row_index FROM \"{}\" WHERE LOWER(\"{}\") = LOWER(?) LIMIT 1",
        table_name, primary_col
    );
    let res: Result<i64, _> = conn.query_row(&query, [display_text], |row| row.get(0));
    match res {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
