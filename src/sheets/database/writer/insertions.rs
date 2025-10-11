// src/sheets/database/writer/insertions.rs
// Insertion operations - adding new rows and grid data

use super::super::error::DbResult;
use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use rusqlite::{params, Connection, Transaction};

/// Insert grid data rows
pub fn insert_grid_data(
    tx: &Transaction,
    table_name: &str,
    grid: &[Vec<String>],
    metadata: &SheetMetadata,
) -> DbResult<()> {
    let column_names: Vec<String> = metadata
        .columns
        .iter()
        .filter(|c| !matches!(c.validator, Some(ColumnValidator::Structure)))
        .map(|c| c.header.clone())
        .collect();

    if column_names.is_empty() || grid.is_empty() {
        return Ok(());
    }

    let placeholders = (0..column_names.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let cols_str = column_names
        .iter()
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
    let column_names: Vec<String> = metadata
        .columns
        .iter()
        .filter(|c| !matches!(c.validator, Some(ColumnValidator::Structure)))
        .map(|c| c.header.clone())
        .collect();

    if column_names.is_empty() || grid.is_empty() {
        return Ok(());
    }

    let placeholders = (0..column_names.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let cols_str = column_names
        .iter()
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

/// Insert a new row
pub fn insert_row(
    conn: &Connection,
    table_name: &str,
    row_data: &[String],
    column_names: &[String],
) -> DbResult<i64> {
    // Get max row_index
    let max_row: i32 = conn.query_row(
        &format!(
            "SELECT COALESCE(MAX(row_index), -1) FROM \"{}\"",
            table_name
        ),
        [],
        |row| row.get(0),
    )?;

    let new_row_index = max_row + 1;

    let placeholders = (0..column_names.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let cols_str = column_names
        .iter()
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
    let placeholders = (0..column_names.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
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
/// OPTIMIZED: Uses a single UPDATE statement instead of row-by-row updates.
/// Handles UNIQUE constraints for structure tables with (parent_id, row_index) uniqueness.
pub fn prepend_row(
    conn: &Connection,
    table_name: &str,
    row_data: &[String],
    column_names: &[String],
) -> DbResult<i64> {
    let tx = conn.unchecked_transaction()?;
    
    // Check if this is a structure table by looking for parent_id column
    let is_structure_table = column_names.iter().any(|name| name == "parent_id");
    
    if is_structure_table {
        // For structure tables with UNIQUE(parent_id, row_index), we need to:
        // 1. Get the parent_id from the data being inserted
        // 2. Only shift rows for that specific parent_id
        // 3. Update in reverse order to avoid constraint violations
        
        // Extract parent_id from row_data (should be first in the data)
        let parent_id = row_data.get(0)
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("parent_id not found or invalid".into()))?;
        
        // Shift rows in DESCENDING order to avoid UNIQUE constraint violations
        // We update from highest row_index to lowest, so we never have conflicts
        {
            let mut stmt = tx.prepare(&format!(
                "SELECT row_index FROM \"{}\" WHERE parent_id = ? ORDER BY row_index DESC",
                table_name
            ))?;
            let existing: Vec<i32> = stmt
                .query_map([parent_id], |r| r.get(0))?
                .collect::<Result<Vec<i32>, _>>()?;
            
            for ri in existing {
                tx.execute(
                    &format!(
                        "UPDATE \"{}\" SET row_index = ?, updated_at = CURRENT_TIMESTAMP WHERE parent_id = ? AND row_index = ?",
                        table_name
                    ),
                    params![ri + 1, parent_id, ri],
                )?;
            }
        }
    } else {
        // For regular tables without compound unique constraints, use fast bulk update
        tx.execute(
            &format!(
                "UPDATE \"{}\" SET row_index = row_index + 1, updated_at = CURRENT_TIMESTAMP",
                table_name
            ),
            [],
        )?;
    }
    
    // Insert new row at index 0
    let inserted = insert_row_with_index(&tx, table_name, 0, row_data, column_names)?;
    tx.commit()?;
    Ok(inserted)
}
