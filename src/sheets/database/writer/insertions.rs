// src/sheets/database/writer/insertions.rs
// Insertion operations - adding new rows and grid data

use super::super::error::DbResult;
use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use rusqlite::{params, Connection, Transaction};
use bevy::prelude::*;

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

/// Append a row at the end (max row_index + 1). No shifting needed - O(1) operation!
/// With DESC sort order, newest rows appear at the top visually.
/// Handles both regular and structure tables efficiently.
pub fn prepend_row(
    conn: &Connection,
    table_name: &str,
    row_data: &[String],
    column_names: &[String],
) -> DbResult<i64> {
    let tx = conn.unchecked_transaction()?;
    
    // Check if this is a structure table by looking for parent_id column
    let is_structure_table = column_names.iter().any(|name| name == "parent_id");
    
    // Find the maximum row_index and insert at max + 1
    let max_index = if is_structure_table {
        // For structure tables, get max row_index for this specific parent_id
        let parent_id = row_data.get(0)
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("parent_id not found or invalid".into()))?;
        
        let max: Option<i32> = tx.query_row(
            &format!("SELECT MAX(row_index) FROM \"{}\" WHERE parent_id = ?", table_name),
            [parent_id],
            |r| r.get(0),
        ).unwrap_or(None);
        
        max.unwrap_or(-1) + 1
    } else {
        // For regular tables, get global max row_index
        let max: Option<i32> = tx.query_row(
            &format!("SELECT MAX(row_index) FROM \"{}\"", table_name),
            [],
            |r| r.get(0),
        ).unwrap_or(None);
        
        let next = max.unwrap_or(-1) + 1;
        info!("prepend_row: table '{}', MAX(row_index)={:?}, using next_index={}", 
              table_name, max, next);
        next
    };
    
    // Insert new row at max_index (no shifting needed!)
    info!("prepend_row: Inserting row with row_index={} into '{}'", max_index, table_name);
    let inserted = insert_row_with_index(&tx, table_name, max_index, row_data, column_names)?;
    tx.commit()?;
    Ok(inserted)
}

/// Batch append multiple rows at the end with single row_index calculation.
/// This prevents race conditions when adding multiple rows - all rows get sequential
/// row_index values starting from max + 1.
/// With DESC sort order, newest rows appear at the top visually (in reverse order of insertion).
pub fn prepend_rows_batch(
    conn: &Connection,
    table_name: &str,
    rows_data: &[Vec<String>],
    column_names: &[String],
) -> DbResult<Vec<i64>> {
    if rows_data.is_empty() {
        return Ok(Vec::new());
    }
    
    let tx = conn.unchecked_transaction()?;
    
    // Check if this is a structure table by looking for parent_id column
    let is_structure_table = column_names.iter().any(|name| name == "parent_id");
    
    // Find the starting row_index (max + 1)
    let start_index = if is_structure_table {
        // For structure tables, get max row_index for this specific parent_id
        // All rows in batch must have same parent_id
        let parent_id = rows_data.get(0)
            .and_then(|row| row.get(0))
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| rusqlite::Error::InvalidParameterName("parent_id not found or invalid".into()))?;
        
        let max: Option<i32> = tx.query_row(
            &format!("SELECT MAX(row_index) FROM \"{}\" WHERE parent_id = ?", table_name),
            [parent_id],
            |r| r.get(0),
        ).unwrap_or(None);
        
        let next = max.unwrap_or(-1) + 1;
        info!("prepend_rows_batch (structure): table '{}', parent_id={}, MAX(row_index)={:?}, using start_index={}", 
              table_name, parent_id, max, next);
        next
    } else {
        // For regular tables, get global max row_index
        let max: Option<i32> = tx.query_row(
            &format!("SELECT MAX(row_index) FROM \"{}\"", table_name),
            [],
            |r| r.get(0),
        ).unwrap_or(None);
        
        // Check if we got NULL (no rows with row_index set)
        let count: i64 = tx.query_row(
            &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
            [],
            |r| r.get(0),
        ).unwrap_or(0);
        
        let next = max.unwrap_or(-1) + 1;
        if max.is_none() && count > 0 {
            warn!("prepend_rows_batch: table '{}' has {} rows but MAX(row_index) is NULL! Row indexes not initialized. Next will be 0.", 
                  table_name, count);
        }
        info!("prepend_rows_batch (regular): table '{}', row_count={}, MAX(row_index)={:?}, using start_index={}", 
              table_name, count, max, next);
        next
    };
    
    // Insert all rows with sequential row_index values
    let mut inserted_ids = Vec::with_capacity(rows_data.len());
    let mut row_indices = Vec::with_capacity(rows_data.len());
    for (i, row_data) in rows_data.iter().enumerate() {
        let row_index = start_index + i as i32;
        let id = insert_row_with_index(&tx, table_name, row_index, row_data, column_names)?;
        inserted_ids.push(id);
        row_indices.push(row_index);
    }
    
    tx.commit()?;
    info!("prepend_rows_batch: Inserted {} rows into '{}' with row_index values: {:?}", 
          rows_data.len(), table_name, row_indices);
    Ok(inserted_ids)
}
