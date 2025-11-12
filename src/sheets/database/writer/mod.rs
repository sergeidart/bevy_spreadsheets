// src/sheets/database/writer/mod.rs
// Main writer module - orchestrates all database write operations

mod insertions;
mod updates;
mod renames;
mod metadata;
mod cascades;
mod helpers;
mod daemon_utils;

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod helpers_tests;

use super::error::DbResult;
use crate::sheets::definitions::{ColumnDataType, ColumnValidator, SheetMetadata};
use rusqlite::{Connection, Transaction};

/// Database writer - provides all write operations
/// 
/// This struct delegates to specialized modules:
/// - `insertions`: Row and grid data insertion
/// - `updates`: Cell and metadata updates
/// - `renames`: Column and table renaming
/// - `metadata`: AI settings and column metadata management
pub struct DbWriter;

impl DbWriter {
    // ============================================================================
    // INSERTIONS - See insertions.rs
    // ============================================================================
    
    /// Insert grid data rows with progress callback
    pub fn insert_grid_data_with_progress<F: FnMut(usize)>(
        tx: &Transaction,
        table_name: &str,
        grid: &[Vec<String>],
        metadata: &SheetMetadata,
        on_chunk: F,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        insertions::insert_grid_data_with_progress(tx, table_name, grid, metadata, on_chunk, daemon_client)
    }

    /// Prepend a row (row_index = 0) by shifting existing rows down
    pub fn prepend_row(
        conn: &Connection,
        table_name: &str,
        row_data: &[String],
        column_names: &[String],
        db_filename: Option<&str>, // Database filename for daemon (e.g., "optima.db")
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<i64> {
        insertions::prepend_row(conn, table_name, row_data, column_names, db_filename, daemon_client)
    }

    /// Batch prepend multiple rows with single row_index calculation
    /// Prevents race conditions when adding multiple rows at once
    pub fn prepend_rows_batch(
        conn: &Connection,
        table_name: &str,
        rows_data: &[Vec<String>],
        column_names: &[String],
        db_filename: Option<&str>, // Database filename for daemon (e.g., "optima.db")
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<Vec<i64>> {
        insertions::prepend_rows_batch(conn, table_name, rows_data, column_names, db_filename, daemon_client)
    }

    // ============================================================================
    // UPDATES - See updates.rs
    // ============================================================================
    
    /// Update a structure sheet's cell value by row id
    pub fn update_structure_cell_by_id(
        conn: &Connection,
        table_name: &str,
        row_id: i64,
        column_name: &str,
        value: &str,
        db_filename: Option<&str>,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        updates::update_structure_cell_by_id(conn, table_name, row_id, column_name, value, db_filename, daemon_client)
    }

    /// Update column ordering in metadata
    pub fn update_column_indices(
        conn: &Connection,
        table_name: &str,
        ordered_pairs: &[(String, i32)],
        db_filename: Option<&str>,
        daemon_client: &super::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        updates::update_column_indices(conn, table_name, ordered_pairs, db_filename, daemon_client)
    }

    // ============================================================================
    // RENAMES - See renames.rs
    // ============================================================================
    
    /// Rename a data column and update its metadata
    pub fn rename_data_column(
        conn: &Connection,
        table_name: &str,
        old_name: &str,
        new_name: &str,
        db_filename: Option<&str>,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        renames::rename_data_column(conn, table_name, old_name, new_name, db_filename, daemon_client)
    }

    /// Atomically rename a structure table and update the parent table's metadata column name.
    /// Both operations succeed or fail together to avoid partial updates.
    pub fn rename_structure_and_parent_metadata_atomic(
        conn: &Connection,
        parent_table: &str,
        old_column_name: &str,
        new_column_name: &str,
        parent_column_index: usize,
        db_filename: Option<&str>,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        use crate::sheets::database::daemon_client::Statement;
        
        // Begin immediate transaction to avoid race conditions
        let begin_stmt = Statement {
            sql: "BEGIN IMMEDIATE".to_string(),
            params: vec![],
        };
        daemon_client.exec_batch(vec![begin_stmt], db_filename)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                e
            ))))?;
            
        let result = (|| -> DbResult<()> {
            // Will no-op if physical structure tables are missing
            renames::rename_structure_table(conn, parent_table, old_column_name, new_column_name, daemon_client)?;
            // Update the parent table's metadata column name
            renames::update_metadata_column_name(
                conn,
                parent_table,
                parent_column_index,
                new_column_name,
                db_filename,
                daemon_client,
            )?;
            // Clean up: if a physical column with the old structure name exists on the parent table, drop it
            // This prevents orphaned physical columns being "recovered" back into metadata on next load.
            let _ = renames::drop_physical_column_if_exists(conn, parent_table, old_column_name, db_filename, daemon_client);
            Ok(())
        })();

        match result {
            Ok(_) => {
                let commit_stmt = Statement {
                    sql: "COMMIT".to_string(),
                    params: vec![],
                };
                daemon_client.exec_batch(vec![commit_stmt], db_filename)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e
                    ))))?;
                Ok(())
            }
            Err(e) => {
                let rollback_stmt = Statement {
                    sql: "ROLLBACK".to_string(),
                    params: vec![],
                };
                let _ = daemon_client.exec_batch(vec![rollback_stmt], None);
                Err(e)
            }
        }
    }

    /// Rename a main table and all descendant structure tables to preserve links after a sheet rename.
    pub fn rename_table_and_descendants(
        conn: &Connection,
        old_table: &str,
        new_table: &str,
        db_filename: Option<&str>,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        renames::rename_table_and_descendants(conn, old_table, new_table, db_filename, daemon_client)
    }

    /// Best-effort: drop a physical column from a table if it exists (SQLite 3.35+).
    pub fn drop_physical_column_if_exists(
        conn: &Connection,
        table_name: &str,
        column_name: &str,
        db_filename: Option<&str>,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        renames::drop_physical_column_if_exists(conn, table_name, column_name, db_filename, daemon_client)
    }

    /// Update parent table's metadata column_name by matching the old column name.
    pub fn update_metadata_column_name_by_name(
        conn: &Connection,
        table_name: &str,
        old_name: &str,
        new_name: &str,
        db_filename: Option<&str>,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        renames::update_metadata_column_name_by_name(conn, table_name, old_name, new_name, db_filename, daemon_client)
    }

    pub fn cascade_key_value_change_to_children(
        conn: &Connection,
        parent_table: &str,
        parent_column_name: &str,
        old_value: &str,
        new_value: &str,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        cascades::cascade_key_value_change_to_children(
            conn,
            parent_table,
            parent_column_name,
            old_value,
            new_value,
            daemon_client,
        )
    }

    pub fn update_table_hidden(
        conn: &Connection,
        table_name: &str,
        hidden: bool,
        db_filename: Option<&str>,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        metadata::update_table_hidden(conn, table_name, hidden, db_filename, daemon_client)
    }

    /// Update table-level AI settings in _Metadata
    pub fn update_table_ai_settings(
        conn: &Connection,
        table_name: &str,
        allow_add_rows: Option<bool>,
        table_context: Option<&str>,
        model_id: Option<&str>,
        active_group: Option<&str>,
        grounding_with_google_search: Option<bool>,
        db_filename: Option<&str>,
        daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        metadata::update_table_ai_settings(
            conn,
            table_name,
            allow_add_rows,
            table_context,
            model_id,
            active_group,
            grounding_with_google_search,
            db_filename,
            daemon_client,
        )
    }

    /// Update a column's metadata (filter, ai_context, ai_include)
    pub fn update_column_metadata(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        filter_expr: Option<&str>,
        ai_context: Option<&str>,
        ai_include_in_send: Option<bool>,
        db_filename: Option<&str>,
        daemon_client: &super::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        metadata::update_column_metadata(
            conn,
            table_name,
            column_index,
            filter_expr,
            ai_context,
            ai_include_in_send,
            db_filename,
            daemon_client,
        )
    }

    /// Update a column's display name (UI-only) in metadata
    pub fn update_column_display_name(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        display_name: &str,
        db_filename: Option<&str>,
        daemon_client: &super::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        metadata::update_column_display_name(conn, table_name, column_index, display_name, db_filename, daemon_client)
    }

    /// Update AI include flag for a column
    pub fn update_column_ai_include(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        include: bool,
        daemon_client: &super::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        metadata::update_column_ai_include(conn, table_name, column_index, include, daemon_client)
    }

    /// Update a column's validator and optional AI flags
    pub fn update_column_validator(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        data_type: ColumnDataType,
        validator: &Option<ColumnValidator>,
        ai_include_in_send: Option<bool>,
        ai_enable_row_generation: Option<bool>,
        db_filename: Option<&str>,
        daemon_client: &super::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        metadata::update_column_validator(
            conn,
            table_name,
            column_index,
            data_type,
            validator,
            ai_include_in_send,
            ai_enable_row_generation,
            db_filename,
            daemon_client,
        )
    }

    /// Add a new column to a table with metadata
    pub fn add_column_with_metadata(
        conn: &Connection,
        table_name: &str,
        column_name: &str,
        data_type: ColumnDataType,
        validator: Option<ColumnValidator>,
        column_index: usize,
        ai_context: Option<&str>,
        filter_expr: Option<&str>,
        ai_enable_row_generation: Option<bool>,
        ai_include_in_send: Option<bool>,
        db_filename: Option<&str>,
        daemon_client: &super::daemon_client::DaemonClient,
    ) -> DbResult<()> {
        metadata::add_column_with_metadata(
            conn,
            table_name,
            column_name,
            data_type,
            validator,
            column_index,
            ai_context,
            filter_expr,
            ai_enable_row_generation,
            ai_include_in_send,
            db_filename,
            daemon_client,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
    use test_helpers::{setup_simple_table, setup_metadata_table};

    #[test]
    fn test_update_column_indices_reorders_without_collision() {
        use super::test_helpers::create_mock_daemon_client;
        
        let conn = Connection::open_in_memory().unwrap();
        let table = "Main";
        setup_metadata_table(&conn, table, &["A", "B", "C", "D"]);

        // New order: D, B, A, C
        let pairs = vec![
            ("D".to_string(), 0),
            ("B".to_string(), 1),
            ("A".to_string(), 2),
            ("C".to_string(), 3),
        ];
        let mock_daemon = create_mock_daemon_client();
        DbWriter::update_column_indices(&conn, table, &pairs, None, &mock_daemon).unwrap();
        // Verify ordering by selecting ordered by column_index
        let meta = format!("{}_Metadata", table);
        let mut stmt = conn
            .prepare(&format!(
                "SELECT column_name FROM \"{}\" ORDER BY column_index",
                meta
            ))
            .unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(cols, vec!["D", "B", "A", "C"]);
    }
    
    // TODO: Re-enable after creating mock daemon client infrastructure
    #[test]
    fn test_prepend_row_shifts_and_inserts() {
        use super::test_helpers::create_mock_daemon_client;
        
        let conn = Connection::open_in_memory().unwrap();
        let table = "Main";
        setup_simple_table(&conn, table);
        
        // Seed two rows: indices 0 => "A0", 1 => "A1"
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (row_index, \"Name\") VALUES (?, ?)",
                table
            ),
            params![0i32, "A0"],
        )
        .unwrap();
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (row_index, \"Name\") VALUES (?, ?)",
                table
            ),
            params![1i32, "A1"],
        )
        .unwrap();
        
        let cols = vec!["Name".to_string()];
        let data = vec!["New".to_string()];
        let mock_daemon = create_mock_daemon_client();
        
        DbWriter::prepend_row(&conn, table, &data, &cols, None, &mock_daemon).unwrap();
        
        let mut stmt = conn
            .prepare(&format!(
                "SELECT row_index, \"Name\" FROM \"{}\" ORDER BY row_index DESC",
                table
            ))
            .unwrap();
        let rows: Vec<(i32, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], (2, "New".to_string())); // Newest at top
        assert_eq!(rows[1], (1, "A1".to_string()));
        assert_eq!(rows[2], (0, "A0".to_string()));
    }
}
