// src/sheets/database/migration/fix_row_index_duplicates.rs

use bevy::prelude::*;
use rusqlite::Connection;

use super::occasional_fixes::MigrationFix;
use super::super::error::DbResult;

/// Fix for duplicate row_index values caused by per-parent indexing bug
/// 
/// Issue: During initial migration, structure tables were assigned row_index
/// per-parent (sidx = 0, 1, 2 for each parent), causing massive duplicates.
/// All tables (parent and child) need unique sequential row_index values.
/// 
/// This fix reassigns row_index = ROW_NUMBER() based on id column order.
pub struct FixRowIndexDuplicates;

impl MigrationFix for FixRowIndexDuplicates {
    fn id(&self) -> &str {
        "fix_row_index_duplicates_2025_10_12"
    }

    fn description(&self) -> &str {
        "Reassign row_index sequentially to fix duplicates from migration bug"
    }

    fn apply(&self, conn: &mut Connection) -> DbResult<()> {
        info!("Starting row_index deduplication fix...");
        
        // Get all table names from global metadata
        let tables: Vec<String> = conn
            .prepare("SELECT table_name FROM GlobalMetadata ORDER BY display_order")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut fixed_count = 0;
        let mut skipped_count = 0;

        for table_name in &tables {
            // Check if table has row_index column
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
                info!("Skipping '{}': no row_index column", table_name);
                skipped_count += 1;
                continue;
            }

            // Check for duplicates
            let duplicate_count: i32 = conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM (
                        SELECT row_index, COUNT(*) as cnt 
                        FROM \"{}\" 
                        GROUP BY row_index 
                        HAVING cnt > 1
                    )",
                    table_name
                ),
                [],
                |row| row.get(0),
            )?;

            let total_rows: i32 = conn.query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
                [],
                |row| row.get(0),
            )?;

            if duplicate_count == 0 && total_rows > 0 {
                // Check if row_index is sequential (0 to n-1)
                let max_idx: Option<i32> = conn.query_row(
                    &format!("SELECT MAX(row_index) FROM \"{}\"", table_name),
                    [],
                    |row| row.get(0),
                )?;
                
                if let Some(max) = max_idx {
                    if max == total_rows - 1 {
                        info!("Skipping '{}': already has sequential row_index (0 to {})", 
                              table_name, max);
                        skipped_count += 1;
                        continue;
                    }
                }
            }

            info!("Fixing '{}': {} rows, {} duplicate row_index values", 
                  table_name, total_rows, duplicate_count);

            // Reassign row_index sequentially based on id order
            // Use a temporary column to avoid conflicts during update
            conn.execute(
                &format!("ALTER TABLE \"{}\" ADD COLUMN temp_new_row_index INTEGER", table_name),
                [],
            ).ok(); // Ignore error if column already exists

            // Calculate new row_index values using ROW_NUMBER() window function
            conn.execute(
                &format!(
                    "UPDATE \"{}\" SET temp_new_row_index = (
                        SELECT COUNT(*) - 1 FROM \"{}\" AS t2 
                        WHERE t2.id <= \"{}\".id
                    )",
                    table_name, table_name, table_name
                ),
                [],
            )?;

            // Copy temp values to actual row_index
            conn.execute(
                &format!(
                    "UPDATE \"{}\" SET row_index = temp_new_row_index",
                    table_name
                ),
                [],
            )?;

            // Drop temporary column
            // Note: SQLite doesn't support DROP COLUMN directly, so we leave it
            // Or recreate table without it (complex, skip for now)

            // Verify fix
            let new_duplicate_count: i32 = conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM (
                        SELECT row_index, COUNT(*) as cnt 
                        FROM \"{}\" 
                        GROUP BY row_index 
                        HAVING cnt > 1
                    )",
                    table_name
                ),
                [],
                |row| row.get(0),
            )?;

            if new_duplicate_count == 0 {
                info!("✓ Fixed '{}': all row_index values now unique", table_name);
                fixed_count += 1;
            } else {
                warn!("⚠ '{}' still has {} duplicates after fix!", 
                      table_name, new_duplicate_count);
            }
        }

        info!("Row_index fix complete: {} tables fixed, {} skipped", 
              fixed_count, skipped_count);
        Ok(())
    }
}
