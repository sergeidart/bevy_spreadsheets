// src/sheets/database/reader/queries.rs
use rusqlite::Connection;
use super::super::error::DbResult;

// NOTE: table_exists() and get_table_type() have been consolidated into schema/queries.rs
// Use super::super::schema::queries::{table_exists, get_table_type} instead

// NOTE: column_exists_in_table() is no longer needed since daemon handles duplicate column checks
// The daemon's exec_alter_table will gracefully handle existing columns

/// Add a column to a table if it doesn't exist
/// NOTE: This is a write operation and should go through the daemon
pub fn add_column_if_missing(
    daemon_client: &super::super::daemon_client::DaemonClient,
    table_name: &str,
    column_name: &str,
    column_type: &str,
    default_value: &str,
) -> DbResult<()> {
    daemon_client
        .exec_alter_table(table_name, column_name, column_type, default_value)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e,
        ))))?;
    bevy::log::info!("Added column '{}' to table '{}'", column_name, table_name);
    Ok(())
}

/// Get physical column information (name, SQL type) from a table
/// Returns both physical (sanitized) and metadata indices
pub fn get_physical_columns(conn: &Connection, table_name: &str) -> DbResult<Vec<(String, String)>> {
    let mut columns = Vec::new();
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
        columns.push(row?);
    }
    
    Ok(columns)
}

/// Get column names from physical table
/// These are the ACTUAL column names in the DB (may differ from visual names)
pub fn get_physical_column_names(conn: &Connection, table_name: &str) -> DbResult<Vec<String>> {
    let mut columns = Vec::new();
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    
    for row in stmt.query_map([], |r| r.get::<_, String>(1))? {
        columns.push(row?);
    }
    
    Ok(columns)
}

/// Read metadata columns from <table>_Metadata table
pub fn read_metadata_columns(
    conn: &Connection,
    meta_table: &str,
) -> DbResult<Vec<MetadataColumnRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT column_index, column_name, display_name, data_type, validator_type, validator_config, 
                ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send, deleted
         FROM \"{}\" ORDER BY column_index",
        meta_table
    ))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(MetadataColumnRow {
                column_index: row.get(0)?,
                column_name: row.get(1)?,
                display_name: row.get(2)?,
                data_type: row.get(3)?,
                validator_type: row.get(4)?,
                validator_config: row.get(5)?,
                ai_context: row.get(6)?,
                filter_expr: row.get(7)?,
                ai_enable_row_generation: row.get(8)?,
                ai_include_in_send: row.get(9)?,
                deleted: row.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// Insert metadata row for an orphaned column
/// NOTE: This is a write operation and should go through the daemon
pub fn insert_orphaned_column_metadata(
    daemon_client: &super::super::daemon_client::DaemonClient,
    meta_table: &str,
    column_index: i32,
    column_name: &str,
    data_type: &str,
) -> DbResult<()> {
    daemon_client
        .exec_insert_metadata(meta_table, column_index, column_name, data_type)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e,
        ))))?;
    Ok(())
}

/// Read table-level metadata (AI settings, category, etc.)
pub fn read_table_metadata(
    conn: &Connection,
    table_name: &str,
) -> DbResult<TableMetadataRow> {
    let row = conn
        .query_row(
            "SELECT ai_allow_add_rows, ai_table_context, ai_model_id, ai_active_group, category, hidden, ai_grounding_with_google_search
             FROM _Metadata WHERE table_name = ?",
            [table_name],
            |row| {
                Ok(TableMetadataRow {
                    ai_allow_add_rows: row.get(0)?,
                    ai_table_context: row.get(1)?,
                    ai_model_id: row.get(2).ok(),
                    ai_active_group: row.get(3)?,
                    category: row.get(4)?,
                    hidden: row.get(5).ok(),
                    ai_grounding: row.get(6).ok(),
                })
            },
        )
        .unwrap_or_else(|_| TableMetadataRow {
            ai_allow_add_rows: 0,
            ai_table_context: None,
            ai_model_id: None,
            ai_active_group: None,
            category: None,
            hidden: None,
            ai_grounding: None,
        });

    Ok(row)
}

/// Read grid data with structure column counts
pub fn read_grid_with_structure_counts(
    conn: &Connection,
    table_name: &str,
    non_structure_cols: &[(usize, String)],
    structure_cols: &[(usize, String)],
) -> DbResult<Vec<GridRow>> {
    if non_structure_cols.is_empty() && structure_cols.is_empty() {
        return Ok(Vec::new());
    }

    // Cast all values to TEXT to avoid type mismatch
    let select_cols = non_structure_cols
        .iter()
        .map(|(_, name)| format!("CAST(\"{}\" AS TEXT) AS \"{}\"", name, name))
        .collect::<Vec<_>>()
        .join(", ");

    let query = format!(
        "SELECT id, row_index, {} FROM \"{}\" ORDER BY CAST(row_index AS INTEGER) DESC",
        select_cols, table_name
    );

    bevy::log::info!("read_grid SQL: {}", query);

    let mut stmt = conn.prepare(&query)?;
    let stmt_col_count = stmt.column_count();

    bevy::log::info!(
        "read_grid: '{}' stmt columns={}, non_structure={}, structure={}",
        table_name,
        stmt_col_count,
        non_structure_cols.len(),
        structure_cols.len()
    );

    let rows = stmt
        .query_map([], |row| {
            let row_id: i64 = row.get(0)?;
            let row_index: i64 = row.get(1)?;

            // Read non-structure column values
            let mut values = Vec::new();
            let max_values = stmt_col_count.saturating_sub(2); // minus id, row_index
            let actual_count = non_structure_cols.len().min(max_values);

            for i in 0..actual_count {
                let value: Option<String> = row.get(i + 2).unwrap_or(None);
                values.push(value.unwrap_or_default());
            }

            // Query structure column counts
            let mut structure_counts = Vec::new();
            for (col_idx, col_name) in structure_cols {
                let structure_table = format!("{}_{}", table_name, col_name);
                let count: i64 = conn
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM \"{}\" WHERE parent_id = ?",
                            structure_table
                        ),
                        [row_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);

                let label = if count == 1 {
                    "1 row".to_string()
                } else {
                    format!("{} rows", count)
                };
                structure_counts.push((*col_idx, label));
            }

            Ok(GridRow {
                row_index,
                values,
                structure_counts,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// List all main and structure tables
pub fn list_all_tables(conn: &Connection) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT table_name FROM _Metadata 
         WHERE table_type IN ('main','structure') OR table_type IS NULL
         ORDER BY display_order, table_name",
    )?;

    let tables = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;

    Ok(tables)
}

// ============================================================================
// Data structures for query results
// ============================================================================

#[derive(Debug)]
pub struct MetadataColumnRow {
    /// Visual column index (display order in UI, from metadata)
    pub column_index: i32,
    pub column_name: String,
    pub display_name: Option<String>,
    pub data_type: String,
    pub validator_type: Option<String>,
    pub validator_config: Option<String>,
    pub ai_context: Option<String>,
    pub filter_expr: Option<String>,
    pub ai_enable_row_generation: Option<i32>,
    pub ai_include_in_send: Option<i32>,
    pub deleted: Option<i32>,
}

impl MetadataColumnRow {
    /// Get physical column index by looking up in actual table schema
    /// Physical index accounts for reordering - it's the actual DB column position
    /// 
    /// # Usage
    /// Call this when you need to map a metadata column_index to its actual position
    /// in the database table, which may differ after column reordering operations.
    /// 
    /// # Example
    /// ```ignore
    /// // When reading data where column order might not match metadata:
    /// let meta_rows = get_metadata_columns(&conn, table_name)?;
    /// for meta_row in &meta_rows {
    ///     if let Some(physical_idx) = meta_row.get_physical_index(&conn, table_name)? {
    ///         // Use physical_idx to access the correct column in query results
    ///     }
    /// }
    /// ```
    pub fn get_physical_index(&self, conn: &Connection, table_name: &str) -> DbResult<Option<usize>> {
        let physical_columns = get_physical_column_names(conn, table_name)?;
        Ok(physical_columns
            .iter()
            .position(|name| name.eq_ignore_ascii_case(&self.column_name)))
    }
}

#[derive(Debug)]
pub struct TableMetadataRow {
    pub ai_allow_add_rows: i32,
    pub ai_table_context: Option<String>,
    pub ai_model_id: Option<String>,
    pub ai_active_group: Option<String>,
    pub category: Option<String>,
    pub hidden: Option<i32>,
    pub ai_grounding: Option<i32>,
}

#[derive(Debug)]
pub struct GridRow {
    pub row_index: i64,
    pub values: Vec<String>,
    pub structure_counts: Vec<(usize, String)>, // (col_idx, count_label)
}
