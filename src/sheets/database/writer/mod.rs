// src/sheets/database/writer/mod.rs
// Main writer module - orchestrates all database write operations

mod insertions;
mod updates;
mod deletions;
mod renames;
mod metadata;
mod cascades;
mod helpers;

#[cfg(test)]
mod test_helpers;

use super::error::DbResult;
use crate::sheets::definitions::{ColumnDataType, ColumnValidator, SheetMetadata};
use rusqlite::{Connection, Transaction};

/// Database writer - provides all write operations
/// 
/// This struct delegates to specialized modules:
/// - `insertions`: Row and grid data insertion
/// - `updates`: Cell and metadata updates
/// - `deletions`: Row deletion and compaction
/// - `renames`: Column and table renaming
/// - `metadata`: AI settings and column metadata management
pub struct DbWriter;

impl DbWriter {
    // ============================================================================
    // INSERTIONS - See insertions.rs
    // ============================================================================
    
    /// Insert grid data rows
    pub fn insert_grid_data(
        tx: &Transaction,
        table_name: &str,
        grid: &[Vec<String>],
        metadata: &SheetMetadata,
    ) -> DbResult<()> {
        insertions::insert_grid_data(tx, table_name, grid, metadata)
    }

    /// Insert grid data rows with progress callback
    pub fn insert_grid_data_with_progress<F: FnMut(usize)>(
        tx: &Transaction,
        table_name: &str,
        grid: &[Vec<String>],
        metadata: &SheetMetadata,
        on_chunk: F,
    ) -> DbResult<()> {
        insertions::insert_grid_data_with_progress(tx, table_name, grid, metadata, on_chunk)
    }

    /// Insert a new row (appends to end)
    pub fn insert_row(
        conn: &Connection,
        table_name: &str,
        row_data: &[String],
        column_names: &[String],
    ) -> DbResult<i64> {
        insertions::insert_row(conn, table_name, row_data, column_names)
    }

    /// Insert a new row at an explicit row_index value
    pub fn insert_row_with_index(
        conn: &Connection,
        table_name: &str,
        row_index: i32,
        row_data: &[String],
        column_names: &[String],
    ) -> DbResult<i64> {
        insertions::insert_row_with_index(conn, table_name, row_index, row_data, column_names)
    }

    /// Prepend a row (row_index = 0) by shifting existing rows down
    pub fn prepend_row(
        conn: &Connection,
        table_name: &str,
        row_data: &[String],
        column_names: &[String],
    ) -> DbResult<i64> {
        insertions::prepend_row(conn, table_name, row_data, column_names)
    }

    /// Batch prepend multiple rows with single row_index calculation
    /// Prevents race conditions when adding multiple rows at once
    pub fn prepend_rows_batch(
        conn: &Connection,
        table_name: &str,
        rows_data: &[Vec<String>],
        column_names: &[String],
    ) -> DbResult<Vec<i64>> {
        insertions::prepend_rows_batch(conn, table_name, rows_data, column_names)
    }

    // ============================================================================
    // UPDATES - See updates.rs
    // ============================================================================
    
    /// Update a single cell
    pub fn update_cell(
        conn: &Connection,
        table_name: &str,
        row_index: usize,
        column_name: &str,
        value: &str,
    ) -> DbResult<()> {
        updates::update_cell(conn, table_name, row_index, column_name, value)
    }

    /// Update a structure sheet's cell value by row id
    pub fn update_structure_cell_by_id(
        conn: &Connection,
        table_name: &str,
        row_id: i64,
        column_name: &str,
        value: &str,
    ) -> DbResult<()> {
        updates::update_structure_cell_by_id(conn, table_name, row_id, column_name, value)
    }

    /// Update column ordering in metadata
    pub fn update_column_indices(
        conn: &Connection,
        table_name: &str,
        ordered_pairs: &[(String, i32)],
    ) -> DbResult<()> {
        updates::update_column_indices(conn, table_name, ordered_pairs)
    }

    // ============================================================================
    // DELETIONS - See deletions.rs
    // ============================================================================
    
    /// Delete a row
    pub fn delete_row(conn: &Connection, table_name: &str, row_index: usize) -> DbResult<()> {
        deletions::delete_row(conn, table_name, row_index)
    }

    /// Delete a row and compact subsequent row_index values
    pub fn delete_row_and_compact(
        conn: &Connection,
        table_name: &str,
        row_index: usize,
    ) -> DbResult<()> {
        deletions::delete_row_and_compact(conn, table_name, row_index)
    }

    /// Delete a structure row by primary key id
    pub fn delete_structure_row_by_id(
        conn: &Connection,
        table_name: &str,
        id: i64,
    ) -> DbResult<()> {
        deletions::delete_structure_row_by_id(conn, table_name, id)
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
    ) -> DbResult<()> {
        renames::rename_data_column(conn, table_name, old_name, new_name)
    }

    /// Update metadata column_name only (for virtual columns)
    pub fn update_metadata_column_name(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        new_name: &str,
    ) -> DbResult<()> {
        renames::update_metadata_column_name(conn, table_name, column_index, new_name)
    }

    /// Rename a structure table and its metadata table
    pub fn rename_structure_table(
        conn: &Connection,
        parent_table: &str,
        old_column_name: &str,
        new_column_name: &str,
    ) -> DbResult<()> {
        renames::rename_structure_table(conn, parent_table, old_column_name, new_column_name)
    }

    /// Atomically rename a structure table and update the parent table's metadata column name.
    /// Both operations succeed or fail together to avoid partial updates.
    pub fn rename_structure_and_parent_metadata_atomic(
        conn: &Connection,
        parent_table: &str,
        old_column_name: &str,
        new_column_name: &str,
        parent_column_index: usize,
    ) -> DbResult<()> {
        // Begin immediate transaction to avoid race conditions
        conn.execute("BEGIN IMMEDIATE", [])?;
        let result = (|| -> DbResult<()> {
            // Will no-op if physical structure tables are missing
            renames::rename_structure_table(conn, parent_table, old_column_name, new_column_name)?;
            // Update the parent table's metadata column name
            renames::update_metadata_column_name(
                conn,
                parent_table,
                parent_column_index,
                new_column_name,
            )?;
            // Clean up: if a physical column with the old structure name exists on the parent table, drop it
            // This prevents orphaned physical columns being "recovered" back into metadata on next load.
            let _ = renames::drop_physical_column_if_exists(conn, parent_table, old_column_name);
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

    /// Rename a main table and all descendant structure tables to preserve links after a sheet rename.
    pub fn rename_table_and_descendants(
        conn: &Connection,
        old_table: &str,
        new_table: &str,
    ) -> DbResult<()> {
        renames::rename_table_and_descendants(conn, old_table, new_table)
    }

    /// Best-effort: drop a physical column from a table if it exists (SQLite 3.35+).
    pub fn drop_physical_column_if_exists(
        conn: &Connection,
        table_name: &str,
        column_name: &str,
    ) -> DbResult<()> {
        renames::drop_physical_column_if_exists(conn, table_name, column_name)
    }

    /// Update parent table's metadata column_name by matching the old column name.
    pub fn update_metadata_column_name_by_name(
        conn: &Connection,
        table_name: &str,
        old_name: &str,
        new_name: &str,
    ) -> DbResult<()> {
        renames::update_metadata_column_name_by_name(conn, table_name, old_name, new_name)
    }

    pub fn cascade_key_value_change_to_children(
        conn: &Connection,
        parent_table: &str,
        parent_column_name: &str,
        old_value: &str,
        new_value: &str,
    ) -> DbResult<()> {
        cascades::cascade_key_value_change_to_children(
            conn,
            parent_table,
            parent_column_name,
            old_value,
            new_value,
        )
    }

    pub fn update_table_hidden(conn: &Connection, table_name: &str, hidden: bool) -> DbResult<()> {
        metadata::update_table_hidden(conn, table_name, hidden)
    }

    /// Update table-level AI settings in _Metadata
    pub fn update_table_ai_settings(
        conn: &Connection,
        table_name: &str,
        allow_add_rows: Option<bool>,
        table_context: Option<&str>,
        active_group: Option<&str>,
        grounding_with_google_search: Option<bool>,
    ) -> DbResult<()> {
        metadata::update_table_ai_settings(
            conn,
            table_name,
            allow_add_rows,
            table_context,
            active_group,
            grounding_with_google_search,
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
    ) -> DbResult<()> {
        metadata::update_column_metadata(
            conn,
            table_name,
            column_index,
            filter_expr,
            ai_context,
            ai_include_in_send,
        )
    }

    /// Update a column's display name (UI-only) in metadata
    pub fn update_column_display_name(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        display_name: &str,
    ) -> DbResult<()> {
        metadata::update_column_display_name(conn, table_name, column_index, display_name)
    }

    /// Update AI include flag for a column
    pub fn update_column_ai_include(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        include: bool,
    ) -> DbResult<()> {
        metadata::update_column_ai_include(conn, table_name, column_index, include)
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
    ) -> DbResult<()> {
        metadata::update_column_validator(
            conn,
            table_name,
            column_index,
            data_type,
            validator,
            ai_include_in_send,
            ai_enable_row_generation,
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
        DbWriter::update_column_indices(&conn, table, &pairs).unwrap();
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
    #[test]
    fn test_update_cell_updates_value() {
        let conn = Connection::open_in_memory().unwrap();
        let table = "Main";
        setup_simple_table(&conn, table);

        // Insert one row directly
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (row_index, \"Name\") VALUES (?, ?)",
                table
            ),
            params![0i32, "Alice"],
        )
        .unwrap();

        // Update via DbWriter
        DbWriter::update_cell(&conn, table, 0usize, "Name", "Bob").unwrap();
        // Verify
        let name: String = conn
            .query_row(
                &format!("SELECT \"Name\" FROM \"{}\" WHERE row_index = 0", table),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Bob");
    }
    #[test]
    fn test_prepend_row_shifts_and_inserts() {
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
        DbWriter::prepend_row(&conn, table, &data, &cols).unwrap();
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
