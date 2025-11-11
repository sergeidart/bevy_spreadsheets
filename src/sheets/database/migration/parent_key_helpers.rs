// src/sheets/database/migration/parent_key_helpers.rs

use rusqlite::{Connection, OptionalExtension};

use super::super::error::DbResult;

/// Struct to hold row update information during migration
#[derive(Default)]
pub struct RowUpdates {
    pub id: i64,
    pub parent_key_new: Option<String>,
    pub ancestor_updates: Vec<(String, String)>, // (column_name, new_value)
}

/// Get all ancestor key columns (grand_*_parent) from a table
pub fn get_ancestor_columns(conn: &Connection, table_name: &str) -> DbResult<Vec<String>> {
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

/// Read the primary display/data column name from the table's metadata.
/// Returns the first non-deleted column_name ordered by column_index.
pub fn get_primary_display_column(conn: &Connection, table_name: &str) -> DbResult<Option<String>> {
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
pub fn resolve_text_row_index_by_meta(
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
