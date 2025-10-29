// src/sheets/database/writer/helpers.rs
// Helper functions for SQL generation and parameter preparation

use rusqlite::{params, Connection, OptionalExtension, ToSql};
use super::super::error::DbResult;

/// Quote a SQL identifier by wrapping it in double quotes.
/// This protects against SQL injection and allows special characters in names.
/// 
/// # Example
/// ```
/// let quoted = quote_identifier("User Name");
/// assert_eq!(quoted, "\"User Name\"");
/// ```
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name)
}

/// Build a comma-separated list of quoted column names.
/// 
/// # Example
/// ```
/// let cols = vec!["Name".to_string(), "Age".to_string()];
/// let quoted = quote_column_list(&cols);
/// assert_eq!(quoted, "\"Name\", \"Age\"");
/// ```
pub fn quote_column_list(columns: &[String]) -> String {
    columns
        .iter()
        .map(|name| quote_identifier(name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build a string of SQL placeholders (?, ?, ?, ...).
/// 
/// # Example
/// ```
/// let placeholders = build_placeholders(3);
/// assert_eq!(placeholders, "?, ?, ?");
/// ```
pub fn build_placeholders(count: usize) -> String {
    (0..count)
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build an INSERT SQL statement.
/// 
/// # Example
/// ```
/// let cols = vec!["Name".to_string(), "Age".to_string()];
/// let sql = build_insert_sql("Users", &cols);
/// assert_eq!(sql, "INSERT INTO \"Users\" (row_index, \"Name\", \"Age\") VALUES (?, ?, ?)");
/// ```
pub fn build_insert_sql(table_name: &str, columns: &[String]) -> String {
    let cols_str = quote_column_list(columns);
    let placeholders = build_placeholders(columns.len());
    
    format!(
        "INSERT INTO {} (row_index, {}) VALUES (?, {})",
        quote_identifier(table_name),
        cols_str,
        placeholders
    )
}

/// Build an UPDATE SQL statement for a single column.
/// 
/// # Example
/// ```
/// let sql = build_update_sql("Users", "Name", "row_index = ?");
/// assert_eq!(sql, "UPDATE \"Users\" SET \"Name\" = ? WHERE row_index = ?");
/// ```
pub fn build_update_sql(table_name: &str, column_name: &str, where_clause: &str) -> String {
    format!(
        "UPDATE {} SET {} = ? WHERE {}",
        quote_identifier(table_name),
        quote_identifier(column_name),
        where_clause
    )
}

/// Get the metadata table name for a given table.
/// 
/// # Example
/// ```
/// let meta = metadata_table_name("Users");
/// assert_eq!(meta, "Users_Metadata");
/// ```
pub fn metadata_table_name(table_name: &str) -> String {
    format!("{}_Metadata", table_name)
}

/// Prepare parameters for SQL execution by boxing values.
/// Adds row_data strings to the params vector.
pub fn append_string_params(
    params: &mut Vec<Box<dyn ToSql>>,
    row_data: &[String],
) {
    for cell in row_data {
        params.push(Box::new(cell.clone()));
    }
}

/// Pad parameters vector with empty strings up to a target length.
/// Useful for ensuring all columns have values even if data is sparse.
pub fn pad_params_with_empty_strings(
    params: &mut Vec<Box<dyn ToSql>>,
    target_len: usize,
) {
    while params.len() < target_len {
        params.push(Box::new(String::new()));
    }
}

/// Get column index from metadata table by column name.
/// Returns None if the column is not found.
pub fn get_column_index_by_name(
    conn: &Connection,
    meta_table: &str,
    column_name: &str,
) -> DbResult<Option<i32>> {
    let result = conn
        .query_row(
            &format!(
                "SELECT column_index FROM \"{}\" WHERE column_name = ?",
                meta_table
            ),
            params![column_name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(result)
}

/// Check if a column name conflicts with an existing column at a different index.
/// Returns (column_index, deleted_flag) if found, None otherwise.
pub fn check_column_name_conflict(
    conn: &Connection,
    meta_table: &str,
    column_name: &str,
) -> DbResult<Option<(i32, i32)>> {
    let result = conn
        .query_row(
            &format!(
                "SELECT column_index, deleted FROM \"{}\" WHERE column_name = ?",
                meta_table
            ),
            params![column_name],
            |row| Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?)),
        )
        .optional()?;
    Ok(result)
}

/// Delete a conflicting deleted column from metadata table.
/// Only deletes rows where deleted = 1.
pub fn delete_conflicting_deleted_column(
    conn: &Connection,
    meta_table: &str,
    column_name: &str,
) -> DbResult<()> {
    conn.execute(
        &format!(
            "DELETE FROM \"{}\" WHERE column_name = ? AND deleted = 1",
            meta_table
        ),
        params![column_name],
    )?;
    Ok(())
}

/// Handle column name conflict by checking if new_name exists at a different index.
/// If it exists and is deleted, removes it. If it exists and is active, returns error.
/// If it exists at the same index, allows it (no-op).
pub fn handle_column_conflict(
    conn: &Connection,
    meta_table: &str,
    table_name: &str,
    new_name: &str,
    source_idx: i32,
) -> DbResult<()> {
    if let Some((existing_idx, is_deleted)) = check_column_name_conflict(conn, meta_table, new_name)? {
        if existing_idx != source_idx {
            // There's a column with this name at a different index
            if is_deleted == 1 {
                // Delete the conflicting deleted metadata row to allow reuse of the name
                bevy::log::warn!(
                    "Found deleted column '{}' at index {} in '{}' - deleting its metadata row to avoid conflict (source index={})",
                    new_name, existing_idx, meta_table, source_idx
                );
                delete_conflicting_deleted_column(conn, meta_table, new_name)?;
            } else {
                // Active column with same name at different index - this is an error
                return Err(super::super::error::DbError::Other(format!(
                    "Column '{}' already exists at index {} in table '{}' (not deleted)",
                    new_name, existing_idx, table_name
                )));
            }
        }
        // If existing_idx == source_idx, that means the column is already named this way (no-op)
    }
    Ok(())
}

/// Rename a table in the database.
/// 
/// # Example
/// ```
/// rename_table(conn, "OldTable", "NewTable")?;
/// ```
pub fn rename_table(conn: &Connection, old_name: &str, new_name: &str) -> DbResult<()> {
    conn.execute(
        &format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", old_name, new_name),
        [],
    )?;
    Ok(())
}

/// Rename a column in a table.
/// 
/// # Example
/// ```
/// rename_column(conn, "Users", "old_name", "new_name")?;
/// ```
pub fn rename_column(
    conn: &Connection,
    table_name: &str,
    old_name: &str,
    new_name: &str,
) -> DbResult<()> {
    conn.execute(
        &format!(
            "ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"",
            table_name, old_name, new_name
        ),
        [],
    )?;
    Ok(())
}

/// Update column_name in metadata table by column_index.
/// Returns the number of rows updated.
/// 
/// # Example
/// ```
/// let updated = update_metadata_column_name_by_index(conn, "Users_Metadata", 5, "NewName")?;
/// ```
pub fn update_metadata_column_name_by_index(
    conn: &Connection,
    meta_table: &str,
    column_index: i32,
    new_name: &str,
) -> DbResult<usize> {
    let count = conn.execute(
        &format!(
            "UPDATE \"{}\" SET column_name = ? WHERE column_index = ?",
            meta_table
        ),
        params![new_name, column_index],
    )?;
    Ok(count)
}

/// Execute a function within a database transaction.
/// Automatically commits on success or rolls back on error.
/// 
/// # Example
/// ```
/// with_transaction(conn, || {
///     // ... database operations
///     Ok(())
/// })?;
/// ```
pub fn with_transaction<F>(conn: &Connection, f: F) -> DbResult<()>
where
    F: FnOnce(&Connection) -> DbResult<()>,
{
    conn.execute("BEGIN IMMEDIATE", [])?;
    let result = f(conn);
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

/// Rename a table triplet: data table, metadata table, and AI groups table (if present).
/// Also updates the global _Metadata entry for the renamed table.
/// Cleans up any orphaned metadata or AI groups tables if the data table doesn't exist.
pub fn rename_table_triplet(
    conn: &Connection,
    old_name: &str,
    new_name: &str,
) -> DbResult<()> {
    use super::super::schema::queries::table_exists;
    
    // If target metadata (or groups) already exists without a real data table, clean it up first
    let new_data_exists = table_exists(conn, new_name)?;

    let new_meta = metadata_table_name(new_name);
    let new_meta_exists = table_exists(conn, &new_meta)?;
    if new_meta_exists && !new_data_exists {
        bevy::log::warn!(
            "Found orphan metadata table '{}' without data table '{}'; dropping before rename.",
            new_meta,
            new_name
        );
        conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", new_meta), [])?;
    }

    let new_groups = format!("{}_AIGroups", new_name);
    let new_groups_exists = table_exists(conn, &new_groups)?;
    if new_groups_exists && !new_data_exists {
        bevy::log::warn!(
            "Found orphan AI groups table '{}' without data table '{}'; dropping before rename.",
            new_groups,
            new_name
        );
        conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", new_groups), [])?;
    }

    // Data table
    let data_exists = table_exists(conn, old_name)?;

    if data_exists {
        rename_table(conn, old_name, new_name)?;
    } else {
        bevy::log::warn!(
            "rename_table_triplet: Data table '{}' not found; skipping data rename.",
            old_name
        );
    }

    // Metadata table
    let old_meta = metadata_table_name(old_name);
    let new_meta = metadata_table_name(new_name);
    let meta_exists = table_exists(conn, &old_meta)?;
    if meta_exists {
        rename_table(conn, &old_meta, &new_meta)?;
    } else {
        bevy::log::warn!(
            "rename_table_triplet: Metadata table '{}' not found; skipping metadata rename.",
            old_meta
        );
    }

    // AI Groups table (optional)
    let old_groups = format!("{}_AIGroups", old_name);
    let new_groups = format!("{}_AIGroups", new_name);
    let groups_exists = table_exists(conn, &old_groups)?;
    if groups_exists {
        rename_table(conn, &old_groups, &new_groups)?;
    }

    // Update global metadata table row for the renamed table, if present
    // Remove any orphaned row for the target name first to avoid UNIQUE constraint violations
    conn.execute(
        "DELETE FROM _Metadata WHERE table_name = ?",
        params![new_name],
    )?;
    conn.execute(
        "UPDATE _Metadata SET table_name = ?, updated_at = CURRENT_TIMESTAMP WHERE table_name = ?",
        params![new_name, old_name],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_identifier() {
        assert_eq!(quote_identifier("Name"), "\"Name\"");
        assert_eq!(quote_identifier("User Name"), "\"User Name\"");
    }

    #[test]
    fn test_quote_column_list() {
        let cols = vec!["Name".to_string(), "Age".to_string()];
        assert_eq!(quote_column_list(&cols), "\"Name\", \"Age\"");
    }

    #[test]
    fn test_build_placeholders() {
        assert_eq!(build_placeholders(0), "");
        assert_eq!(build_placeholders(1), "?");
        assert_eq!(build_placeholders(3), "?, ?, ?");
    }

    #[test]
    fn test_build_insert_sql() {
        let cols = vec!["Name".to_string(), "Age".to_string()];
        let sql = build_insert_sql("Users", &cols);
        assert_eq!(
            sql,
            "INSERT INTO \"Users\" (row_index, \"Name\", \"Age\") VALUES (?, ?, ?)"
        );
    }

    #[test]
    fn test_build_update_sql() {
        let sql = build_update_sql("Users", "Name", "row_index = ?");
        assert_eq!(sql, "UPDATE \"Users\" SET \"Name\" = ? WHERE row_index = ?");
    }

    #[test]
    fn test_metadata_table_name() {
        assert_eq!(metadata_table_name("Users"), "Users_Metadata");
        assert_eq!(metadata_table_name("Games_Items"), "Games_Items_Metadata");
    }

    #[test]
    fn test_pad_params_with_empty_strings() {
        let mut params: Vec<Box<dyn ToSql>> = vec![Box::new("test".to_string())];
        pad_params_with_empty_strings(&mut params, 4);
        assert_eq!(params.len(), 4);
    }
}
