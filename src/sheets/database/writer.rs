// src/sheets/database/writer.rs

use rusqlite::{Connection, params, Transaction};
use super::error::DbResult;
use super::schema::sql_type_for_column;
use crate::sheets::definitions::{SheetMetadata, ColumnValidator, ColumnDataType};

pub struct DbWriter;

impl DbWriter {
    /// Update a table's hidden flag in the global _Metadata table
    pub fn update_table_hidden(conn: &Connection, table_name: &str, hidden: bool) -> DbResult<()> {
        conn.execute(
            "INSERT INTO _Metadata (table_name, hidden) VALUES (?, ?) \
             ON CONFLICT(table_name) DO UPDATE SET hidden = excluded.hidden, updated_at = CURRENT_TIMESTAMP",
            params![table_name, hidden as i32],
        )?;
        Ok(())
    }

    /// Insert grid data rows
    pub fn insert_grid_data(
        tx: &Transaction,
        table_name: &str,
        grid: &[Vec<String>],
        metadata: &SheetMetadata,
    ) -> DbResult<()> {
        let column_names: Vec<String> = metadata.columns.iter()
            .filter(|c| !matches!(c.validator, Some(ColumnValidator::Structure)))
            .map(|c| c.header.clone())
            .collect();
        
        if column_names.is_empty() || grid.is_empty() {
            return Ok(());
        }
        
        let placeholders = (0..column_names.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let cols_str = column_names.iter()
            .map(|name| format!("\"{}\"", name))
            .collect::<Vec<_>>()
            .join(", ");
        
        let insert_sql = format!(
            "INSERT INTO \"{}\" (row_index, {}) VALUES (?, {})",
            table_name, cols_str, placeholders
        );
        
        let mut stmt = tx.prepare(&insert_sql)?;
        
        for (row_idx, row_data) in grid.iter().enumerate() {
            let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(row_idx as i32)];
            
            for cell in row_data.iter().take(column_names.len()) {
                params_vec.push(Box::new(cell.clone()));
            }
            
            // Fill missing columns with empty strings
            while params_vec.len() <= column_names.len() {
                params_vec.push(Box::new(String::new()));
            }
            
            stmt.execute(rusqlite::params_from_iter(params_vec.iter()))?;
        }
        
        Ok(())
    }

    /// Insert grid data rows and invoke a progress callback after each 1,000 rows.
    pub fn insert_grid_data_with_progress<F: FnMut(usize)>(
        tx: &Transaction,
        table_name: &str,
        grid: &[Vec<String>],
        metadata: &SheetMetadata,
        mut on_chunk: F,
    ) -> DbResult<()> {
        let column_names: Vec<String> = metadata.columns.iter()
            .filter(|c| !matches!(c.validator, Some(ColumnValidator::Structure)))
            .map(|c| c.header.clone())
            .collect();

        if column_names.is_empty() || grid.is_empty() {
            return Ok(());
        }

        let placeholders = (0..column_names.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let cols_str = column_names.iter()
            .map(|name| format!("\"{}\"", name))
            .collect::<Vec<_>>()
            .join(", ");

        let insert_sql = format!(
            "INSERT INTO \"{}\" (row_index, {}) VALUES (?, {})",
            table_name, cols_str, placeholders
        );

        let mut stmt = tx.prepare(&insert_sql)?;

        for (row_idx, row_data) in grid.iter().enumerate() {
            let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(row_idx as i32)];
            for cell in row_data.iter().take(column_names.len()) {
                params_vec.push(Box::new(cell.clone()));
            }
            while params_vec.len() <= column_names.len() {
                params_vec.push(Box::new(String::new()));
            }
            stmt.execute(rusqlite::params_from_iter(params_vec.iter()))?;

            if row_idx > 0 && row_idx % 1000 == 0 {
                on_chunk(row_idx);
            }
        }

        Ok(())
    }
    
    /// Update a single cell
    pub fn update_cell(
        conn: &Connection,
        table_name: &str,
        row_index: usize,
        column_name: &str,
        value: &str,
    ) -> DbResult<()> {
        conn.execute(
            &format!(
                "UPDATE \"{}\" SET \"{}\" = ?, updated_at = CURRENT_TIMESTAMP WHERE row_index = ?",
                table_name, column_name
            ),
            params![value, row_index as i32],
        )?;
        Ok(())
    }
    
    /// Insert a new row
    pub fn insert_row(
        conn: &Connection,
        table_name: &str,
        row_data: &[String],
        column_names: &[String],
    ) -> DbResult<i64> {
        // Get max row_index
        let max_row: i32 = conn.query_row(
            &format!("SELECT COALESCE(MAX(row_index), -1) FROM \"{}\"", table_name),
            [],
            |row| row.get(0),
        )?;
        
        let new_row_index = max_row + 1;
        
        let placeholders = (0..column_names.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let cols_str = column_names.iter()
            .map(|name| format!("\"{}\"", name))
            .collect::<Vec<_>>()
            .join(", ");
        
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(new_row_index)];
        for cell in row_data {
            params_vec.push(Box::new(cell.clone()));
        }
        
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (row_index, {}) VALUES (?, {})",
                table_name, cols_str, placeholders
            ),
            rusqlite::params_from_iter(params_vec.iter()),
        )?;
        
        Ok(conn.last_insert_rowid())
    }

    /// Insert a new row at an explicit row_index value.
    /// Note: Caller must ensure row_index uniqueness.
    pub fn insert_row_with_index(
        conn: &Connection,
        table_name: &str,
        row_index: i32,
        row_data: &[String],
        column_names: &[String],
    ) -> DbResult<i64> {
        let placeholders = (0..column_names.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let cols_str = column_names
            .iter()
            .map(|name| format!("\"{}\"", name))
            .collect::<Vec<_>>()
            .join(", ");

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(row_index)];
        for cell in row_data {
            params_vec.push(Box::new(cell.clone()));
        }

        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (row_index, {}) VALUES (?, {})",
                table_name, cols_str, placeholders
            ),
            rusqlite::params_from_iter(params_vec.iter()),
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Prepend a row (row_index = 0) by shifting existing rows down by 1, within a transaction.
    pub fn prepend_row(
        conn: &Connection,
        table_name: &str,
        row_data: &[String],
        column_names: &[String],
    ) -> DbResult<i64> {
        let tx = conn.unchecked_transaction()?;
        {
            // Scope for statement so it drops before commit
            let mut idx_stmt = tx.prepare(&format!("SELECT row_index FROM \"{}\" ORDER BY row_index DESC", table_name))?;
            let existing: Vec<i32> = idx_stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<i32>, _>>()?;
            for ri in existing {
                tx.execute(
                    &format!("UPDATE \"{}\" SET row_index = ? WHERE row_index = ?", table_name),
                    params![ri + 1, ri],
                )?;
            }
        }
        // Insert new row at index 0
        let inserted = Self::insert_row_with_index(&tx, table_name, 0, row_data, column_names)?;
        tx.commit()?;
        Ok(inserted)
    }
    
    /// Delete a row
    pub fn delete_row(
        conn: &Connection,
        table_name: &str,
        row_index: usize,
    ) -> DbResult<()> {
        conn.execute(
            &format!("DELETE FROM \"{}\" WHERE row_index = ?", table_name),
            params![row_index as i32],
        )?;
        Ok(())
    }

    /// Update table-level flags in _Metadata
    pub fn update_table_ai_settings(
        conn: &Connection,
        table_name: &str,
        allow_add_rows: Option<bool>,
        table_context: Option<&str>,
        active_group: Option<&str>,
        grounding_with_google_search: Option<bool>,
    ) -> DbResult<()> {
        // Build dynamic SQL to only update provided fields
        let mut sets: Vec<&str> = Vec::new();
        if allow_add_rows.is_some() { sets.push("ai_allow_add_rows = ?"); }
        if table_context.is_some() { sets.push("ai_table_context = ?"); }
        if active_group.is_some() { sets.push("ai_active_group = ?"); }
        if grounding_with_google_search.is_some() { sets.push("ai_grounding_with_google_search = ?"); }
        if sets.is_empty() { return Ok(()); }
        let sql = format!(
            "UPDATE _Metadata SET {} , updated_at = CURRENT_TIMESTAMP WHERE table_name = ?",
            sets.join(", ")
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(v) = allow_add_rows { params_vec.push(Box::new(v as i32)); }
        if let Some(v) = table_context { params_vec.push(Box::new(v.to_string())); }
        if let Some(v) = active_group { params_vec.push(Box::new(v.to_string())); }
        if let Some(v) = grounding_with_google_search { params_vec.push(Box::new(v as i32)); }
        params_vec.push(Box::new(table_name.to_string()));
        conn.execute(&sql, rusqlite::params_from_iter(params_vec.iter()))?;
        Ok(())
    }

    /// Update a column's filter, ai_context, and include flag in the table's metadata table
    pub fn update_column_metadata(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        filter_expr: Option<&str>,
        ai_context: Option<&str>,
        ai_include_in_send: Option<bool>,
    ) -> DbResult<()> {
        let meta_table = format!("{}_Metadata", table_name);
        let mut sets: Vec<&str> = Vec::new();
        if filter_expr.is_some() { sets.push("filter_expr = ?"); }
        if ai_context.is_some() { sets.push("ai_context = ?"); }
        if ai_include_in_send.is_some() { sets.push("ai_include_in_send = ?"); }
        if sets.is_empty() { return Ok(()); }
        let sql = format!(
            "UPDATE \"{}\" SET {} WHERE column_index = ?",
            meta_table,
            sets.join(", ")
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(v) = filter_expr { params_vec.push(Box::new(v.to_string())); }
        if let Some(v) = ai_context { params_vec.push(Box::new(v.to_string())); }
        if let Some(v) = ai_include_in_send { params_vec.push(Box::new(v as i32)); }
        params_vec.push(Box::new(column_index as i32));
        conn.execute(&sql, rusqlite::params_from_iter(params_vec.iter()))?;
        Ok(())
    }

    /// Update a column's validator (data_type, validator_type, validator_config) and optional AI flags in metadata
    pub fn update_column_validator(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        data_type: ColumnDataType,
        validator: &Option<ColumnValidator>,
        ai_include_in_send: Option<bool>,
        ai_enable_row_generation: Option<bool>,
    ) -> DbResult<()> {
        let meta_table = format!("{}_Metadata", table_name);
        let (validator_type, validator_config): (Option<String>, Option<String>) = match validator {
            Some(ColumnValidator::Basic(_)) => (Some("Basic".to_string()), None),
            Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                let cfg = serde_json::json!({
                    "target_table": target_sheet_name,
                    "target_column_index": target_column_index
                }).to_string();
                (Some("Linked".to_string()), Some(cfg))
            }
            Some(ColumnValidator::Structure) => {
                // Persist structure reference for completeness
                let cfg = serde_json::json!({
                    "structure_table": format!("{}_{}", table_name, "")
                }).to_string();
                (Some("Structure".to_string()), Some(cfg))
            }
            None => (None, None),
        };

        // Build dynamic SQL to include optional AI flags only when provided
        // Note: Metadata tables don't have an updated_at column; only main data tables do.
        // Keep the payload updates minimal and valid for the metadata schema.
        let mut sets = vec![
            "data_type = ?",
            "validator_type = ?",
            "validator_config = ?",
        ];
        if ai_include_in_send.is_some() { sets.push("ai_include_in_send = ?"); }
        if ai_enable_row_generation.is_some() { sets.push("ai_enable_row_generation = ?"); }
        let sql = format!(
            "UPDATE \"{}\" SET {} WHERE column_index = ?",
            meta_table,
            sets.join(", ")
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        params_vec.push(Box::new(format!("{:?}", data_type)));
        params_vec.push(Box::new(validator_type));
        params_vec.push(Box::new(validator_config));
        if let Some(v) = ai_include_in_send { params_vec.push(Box::new(v as i32)); }
        if let Some(v) = ai_enable_row_generation { params_vec.push(Box::new(v as i32)); }
        params_vec.push(Box::new(column_index as i32));

        conn.execute(&sql, rusqlite::params_from_iter(params_vec.iter()))?;
        Ok(())
    }

    /// Update the order (column_index) for columns in the table's metadata table.
    /// Pairs are (column_name, new_index). This updates metadata only; no physical reorder of table columns.
    pub fn update_column_indices(
        conn: &Connection,
        table_name: &str,
        ordered_pairs: &[(String, i32)],
    ) -> DbResult<()> {
        let meta_table = format!("{}_Metadata", table_name);
        let tx = conn.unchecked_transaction()?;
        // Phase 1: Shift all indices by a large offset to avoid UNIQUE collisions during remap
        tx.execute(
            &format!(
                "UPDATE \"{}\" SET column_index = column_index + 10000",
                meta_table
            ),
            [],
        )?;

        // Phase 2: Apply final indices
        {
            let mut stmt = tx.prepare(&format!(
                "UPDATE \"{}\" SET column_index = ? WHERE column_name = ?",
                meta_table
            ))?;
            for (name, idx) in ordered_pairs {
                stmt.execute(params![idx, name])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Rename a data column and update its metadata column_name accordingly (for main or structure tables with real columns).
    pub fn rename_data_column(
        conn: &Connection,
        table_name: &str,
        old_name: &str,
        new_name: &str,
    ) -> DbResult<()> {
        // Rename column in the data table
        conn.execute(
            &format!(
                "ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"",
                table_name, old_name, new_name
            ),
            [],
        )?;
        // Update metadata row
        let meta_table = format!("{}_Metadata", table_name);
        conn.execute(
            &format!(
                "UPDATE \"{}\" SET column_name = ? WHERE column_name = ?",
                meta_table
            ),
            params![new_name, old_name],
        )?;
        Ok(())
    }

    /// Update metadata column_name only (for columns that don't exist physically in main table, e.g., Structure validators)
    pub fn update_metadata_column_name(
        conn: &Connection,
        table_name: &str,
        column_index: usize,
        new_name: &str,
    ) -> DbResult<()> {
        let meta_table = format!("{}_Metadata", table_name);
        conn.execute(
            &format!(
                "UPDATE \"{}\" SET column_name = ? WHERE column_index = ?",
                meta_table
            ),
            params![new_name, column_index as i32],
        )?;
        Ok(())
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
            &format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", old_struct, new_struct),
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

    /// Add a new column to a table (main or structure) and insert its metadata row with given index.
    pub fn add_column_with_metadata(
        conn: &Connection,
        table_name: &str,
        column_name: &str,
        data_type: crate::sheets::definitions::ColumnDataType,
        validator: Option<crate::sheets::definitions::ColumnValidator>,
        column_index: usize,
        ai_context: Option<&str>,
        filter_expr: Option<&str>,
        ai_enable_row_generation: Option<bool>,
        ai_include_in_send: Option<bool>,
    ) -> DbResult<()> {
        // ALTER TABLE ADD COLUMN
        let sql_type = sql_type_for_column(data_type);
        conn.execute(
            &format!("ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}", table_name, column_name, sql_type),
            [],
        )?;

        // Insert metadata row
        let meta_table = format!("{}_Metadata", table_name);
        let (validator_type, validator_config): (Option<String>, Option<String>) = match &validator {
            Some(ColumnValidator::Basic(_)) => (Some("Basic".to_string()), None),
            Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) => {
                let cfg = serde_json::json!({
                    "target_table": target_sheet_name,
                    "target_column_index": target_column_index
                }).to_string();
                (Some("Linked".to_string()), Some(cfg))
            }
            Some(ColumnValidator::Structure) => (Some("Structure".to_string()), Some(serde_json::json!({"structure_table": format!("{}_{}", table_name, column_name)}).to_string())),
            None => (None, None),
        };
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO \"{}\" (column_index, column_name, data_type, validator_type, validator_config, ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                meta_table
            ),
            params![
                column_index as i32,
                column_name,
                format!("{:?}", data_type),
                validator_type,
                validator_config,
                ai_context,
                filter_expr,
                ai_enable_row_generation.unwrap_or(false) as i32,
                ai_include_in_send.unwrap_or(true) as i32
            ],
        )?;
        Ok(())
    }

    /// Update a structure sheet's cell value by row id.
    pub fn update_structure_cell_by_id(
        conn: &Connection,
        table_name: &str,
        row_id: i64,
        column_name: &str,
        value: &str,
    ) -> DbResult<()> {
        conn.execute(
            &format!(
                "UPDATE \"{}\" SET \"{}\" = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                table_name, column_name
            ),
            params![value, row_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_simple_table(conn: &Connection, table: &str) {
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                row_index INTEGER NOT NULL UNIQUE,
                \"Name\" TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            table
        );
        conn.execute(&sql, []).unwrap();
        // Helpful index similar to production
        conn.execute(
            &format!("CREATE INDEX IF NOT EXISTS idx_{}_row_index ON \"{}\"(row_index)", table, table),
            [],
        )
        .unwrap();
    }

    fn setup_metadata_table(conn: &Connection, table: &str, cols: &[&str]) {
        let meta = format!("{}_Metadata", table);
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS \"{}\" (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    column_index INTEGER UNIQUE NOT NULL,
                    column_name TEXT UNIQUE NOT NULL,
                    data_type TEXT,
                    validator_type TEXT,
                    validator_config TEXT,
                    ai_context TEXT,
                    filter_expr TEXT,
                    ai_enable_row_generation INTEGER DEFAULT 0,
                    ai_include_in_send INTEGER DEFAULT 1,
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
                )",
                meta
            ),
            [],
        )
        .unwrap();
        let mut idx = 0i32;
        for c in cols {
            conn.execute(
                &format!("INSERT INTO \"{}\" (column_index, column_name) VALUES (?, ?)", meta),
                params![idx, *c],
            )
            .unwrap();
            idx += 1;
        }
    }

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
            .prepare(&format!("SELECT column_name FROM \"{}\" ORDER BY column_index", meta))
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
            &format!("INSERT INTO \"{}\" (row_index, \"Name\") VALUES (?, ?)", table),
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
            &format!("INSERT INTO \"{}\" (row_index, \"Name\") VALUES (?, ?)", table),
            params![0i32, "A0"],
        )
        .unwrap();
        conn.execute(
            &format!("INSERT INTO \"{}\" (row_index, \"Name\") VALUES (?, ?)", table),
            params![1i32, "A1"],
        )
        .unwrap();

        // Prepend new row "New"
        let cols = vec!["Name".to_string()];
        let data = vec!["New".to_string()];
        DbWriter::prepend_row(&conn, table, &data, &cols).unwrap();

        // Expect three rows with indices 0,1,2 and values New, A0, A1
        let mut stmt = conn
            .prepare(&format!("SELECT row_index, \"Name\" FROM \"{}\" ORDER BY row_index", table))
            .unwrap();
        let rows: Vec<(i32, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], (0, "New".to_string()));
        assert_eq!(rows[1], (1, "A0".to_string()));
        assert_eq!(rows[2], (2, "A1".to_string()));
    }
}
