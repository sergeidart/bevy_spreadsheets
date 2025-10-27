// src/sheets/database/migration/hide_temp_new_row_index_in_metadata.rs

use bevy::prelude::*;
use rusqlite::Connection;

use super::occasional_fixes::MigrationFix;
use super::super::error::DbResult;

/// Marks 'temp_new_row_index' and '_obsolete_temp_new_row_index' as deleted in per-table metadata
/// so they do not show up in the UI and are safe to repurpose later.
pub struct HideTempNewRowIndexInMetadata;

impl MigrationFix for HideTempNewRowIndexInMetadata {
    fn id(&self) -> &str {
        "hide_temp_new_row_index_in_metadata_2025_10_27"
    }

    fn description(&self) -> &str {
        "Mark temporary row index columns as deleted in metadata"
    }

    fn apply(&self, conn: &mut Connection) -> DbResult<()> {
        info!("Hiding temp row index columns in metadata...");

        // Get all table names from global metadata
        let tables: Vec<String> = conn
            .prepare("SELECT table_name FROM _Metadata ORDER BY display_order")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut affected = 0usize;

        for table_name in &tables {
            let meta_table = format!("{}_Metadata", table_name);

            let updated = conn.execute(
                &format!(
                    "UPDATE \"{}\" SET deleted = 1 WHERE LOWER(column_name) IN ('temp_new_row_index','_obsolete_temp_new_row_index')",
                    meta_table
                ),
                [],
            )?;

            if updated > 0 {
                affected += 1;
                info!(
                    "  ✓ Marked temp columns deleted in '{}' ({} row(s))",
                    meta_table, updated
                );
            }
        }

        info!("Hidden temp columns in metadata for {} table(s)", affected);
        Ok(())
    }
}

