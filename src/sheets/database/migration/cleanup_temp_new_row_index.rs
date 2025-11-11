// src/sheets/database/migration/cleanup_temp_new_row_index.rs

use bevy::prelude::*;
use rusqlite::Connection;

use super::occasional_fixes::MigrationFix;
use super::super::error::{DbError, DbResult};

/// Cleanup fix to remove the temporary staging column `temp_new_row_index`
/// introduced by the row_index de-duplication migration.
///
/// Strategy:
/// - If SQLite version >= 3.35.0, issue `ALTER TABLE DROP COLUMN`.
/// - Otherwise, fall back to renaming it to `_obsolete_temp_new_row_index` and
///   rely on UI to hide it (and keep option to repurpose later).
pub struct CleanupTempNewRowIndex;

impl CleanupTempNewRowIndex {
    fn sqlite_version(conn: &Connection) -> DbResult<(i32, i32, i32)> {
        let v: String = conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
        // Expected formats like "3.45.2"; be liberal in parsing
        let mut parts = v.split('.');
        let major = parts.next().unwrap_or("3").parse::<i32>().unwrap_or(3);
        let minor = parts.next().unwrap_or("0").parse::<i32>().unwrap_or(0);
        let patch = parts.next().unwrap_or("0").parse::<i32>().unwrap_or(0);
        Ok((major, minor, patch))
    }

    fn has_temp_column(conn: &Connection, table_name: &str, col: &str) -> DbResult<bool> {
        let count: i32 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?1",
                table_name
            ),
            [col],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

impl MigrationFix for CleanupTempNewRowIndex {
    fn id(&self) -> &str {
        "cleanup_temp_new_row_index_2025_10_27"
    }

    fn description(&self) -> &str {
        "Drop or hide the temporary column 'temp_new_row_index' left by previous migration"
    }

    fn apply(&self, conn: &mut Connection, daemon_client: &super::super::daemon_client::DaemonClient) -> DbResult<()> {
        use super::super::daemon_client::Statement;
        
        info!("Starting cleanup of 'temp_new_row_index' columns...");

        // Get all table names from global metadata
        let tables: Vec<String> = conn
            .prepare("SELECT table_name FROM _Metadata ORDER BY display_order")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let (major, minor, _patch) = Self::sqlite_version(conn)?;
        let can_drop_column = major > 3 || (major == 3 && minor >= 35);
        info!(
            "SQLite version detected: {}.{}; DROP COLUMN supported: {}",
            major, minor, can_drop_column
        );

        let mut dropped = 0usize;
        let mut renamed = 0usize;
        let mut skipped = 0usize;

        for table_name in &tables {
            let has_temp = Self::has_temp_column(conn, table_name, "temp_new_row_index")?;
            if !has_temp {
                skipped += 1;
                continue;
            }

            if can_drop_column {
                let drop_stmt = Statement {
                    sql: format!(
                        "ALTER TABLE \"{}\" DROP COLUMN temp_new_row_index",
                        table_name
                    ),
                    params: vec![],
                };
                match daemon_client.exec_batch(vec![drop_stmt], None) {
                    Ok(_) => {
                        info!("Dropped column 'temp_new_row_index' from '{}'", table_name);
                        dropped += 1;
                        continue;
                    }
                    Err(e) => {
                        warn!(
                            "Failed to drop column from '{}': {}. Will try rename fallback.",
                            table_name, e
                        );
                    }
                }
            }

            // Fallback: rename to mark obsolete (SQLite supports rename since 3.25.0)
            let rename_stmt = Statement {
                sql: format!(
                    "ALTER TABLE \"{}\" RENAME COLUMN temp_new_row_index TO _obsolete_temp_new_row_index",
                    table_name
                ),
                params: vec![],
            };
            match daemon_client.exec_batch(vec![rename_stmt], None) {
                Ok(_) => {
                    info!(
                        "Renamed 'temp_new_row_index' -> '_obsolete_temp_new_row_index' in '{}'",
                        table_name
                    );
                    renamed += 1;
                }
                Err(e) => {
                    return Err(DbError::MigrationFailed(format!(
                        "Failed to drop or rename temp column in '{}': {}",
                        table_name, e
                    )));
                }
            }
        }

        info!(
            "Cleanup complete: {} dropped, {} renamed, {} tables skipped",
            dropped, renamed, skipped
        );
        Ok(())
    }
}

