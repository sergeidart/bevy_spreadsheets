// src/sheets/database/writer/insertions.rs
// Insertion operations - adding new rows and grid data

use super::super::error::DbResult;
use super::helpers::{build_insert_sql, pad_params_with_empty_strings, append_string_params};
use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use rusqlite::{Connection, Transaction};
use bevy::prelude::*;

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

    let insert_sql = build_insert_sql(table_name, &column_names);
    let mut stmt = tx.prepare(&insert_sql)?;

    for (row_idx, row_data) in grid.iter().enumerate() {
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(row_idx as i32)];
        
        // Add cell values (up to column_names length)
        for cell in row_data.iter().take(column_names.len()) {
            params_vec.push(Box::new(cell.clone()));
        }
        
        // Fill missing columns with empty strings (target: 1 + column_names.len())
        pad_params_with_empty_strings(&mut params_vec, column_names.len() + 1);
        
        stmt.execute(rusqlite::params_from_iter(params_vec.iter()))?;

        if row_idx > 0 && row_idx % 1000 == 0 {
            on_chunk(row_idx);
        }
    }

    Ok(())
}

/// Insert a new row at an explicit row_index value (internal helper).
/// Note: Caller must ensure row_index uniqueness.
fn insert_row_with_index(
    conn: &Connection,
    table_name: &str,
    row_index: i32,
    row_data: &[String],
    column_names: &[String],
) -> DbResult<i64> {
    let insert_sql = build_insert_sql(table_name, column_names);
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(row_index)];
    append_string_params(&mut params_vec, row_data);

    conn.execute(&insert_sql, rusqlite::params_from_iter(params_vec.iter()))?;

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
    
    // Check if this is a structure table by looking for parent_key column
    let is_structure_table = column_names.iter().any(|name| name == "parent_key");
    
    // Find the maximum row_index and insert at max + 1
    // IMPORTANT: row_index must be globally unique across the entire table,
    // not per-parent for structure tables, to prevent collisions
    let max_index = {
        // Always use global MAX(row_index) for all table types
        let max: Option<i32> = tx.query_row(
            &format!("SELECT MAX(row_index) FROM \"{}\"", table_name),
            [],
            |r| r.get(0),
        ).unwrap_or(None);
        
        let next = max.unwrap_or(-1) + 1;
        if is_structure_table {
            let parent_key = row_data.iter()
                .zip(column_names.iter())
                .find(|(_, name)| *name == "parent_key")
                .map(|(val, _)| val.as_str())
                .unwrap_or("");
            info!("prepend_row (structure): table '{}', parent_key='{}', global MAX(row_index)={:?}, using next_index={}", 
                  table_name, parent_key, max, next);
        } else {
            info!("prepend_row (regular): table '{}', MAX(row_index)={:?}, using next_index={}", 
                  table_name, max, next);
        }
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
    
    // Check if this is a structure table by looking for parent_key column
    let is_structure_table = column_names.iter().any(|name| name == "parent_key");
    
    // Find the starting row_index (max + 1)
    // IMPORTANT: row_index must be globally unique across the entire table,
    // not per-parent for structure tables, to prevent collisions
    let start_index = {
        // Always use global MAX(row_index) for all table types
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
        
        if is_structure_table {
            let parent_key = rows_data.get(0)
                .and_then(|row| {
                    row.iter()
                        .zip(column_names.iter())
                        .find(|(_, name)| *name == "parent_key")
                        .map(|(val, _)| val.as_str())
                })
                .unwrap_or("");
            info!("prepend_rows_batch (structure): table '{}', parent_key='{}', global MAX(row_index)={:?}, using start_index={}", 
                  table_name, parent_key, max, next);
        } else {
            if max.is_none() && count > 0 {
                warn!("prepend_rows_batch: table '{}' has {} rows but MAX(row_index) is NULL! Row indexes not initialized. Next will be 0.", 
                      table_name, count);
            }
            info!("prepend_rows_batch (regular): table '{}', row_count={}, MAX(row_index)={:?}, using start_index={}", 
                  table_name, count, max, next);
        }
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
