// src/sheets/database/schema/queries.rs
use rusqlite::{params, Connection, OptionalExtension};
use super::super::error::DbResult;

/// Check if a table exists in the database
pub fn table_exists(conn: &Connection, table_name: &str) -> DbResult<bool> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            [table_name],
            |row| row.get::<_, i32>(0).map(|v| v > 0),
        )
        .unwrap_or(false);
    Ok(exists)
}

/// Get list of existing columns in a table
pub fn get_table_columns(conn: &Connection, table_name: &str) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns)
}

/// Check if a column exists in a table
pub fn column_exists(conn: &Connection, table_name: &str, column_name: &str) -> DbResult<bool> {
    let columns = get_table_columns(conn, table_name)?;
    Ok(columns.iter().any(|c| c.eq_ignore_ascii_case(column_name)))
}

/// Add a column to a table if it doesn't exist
pub fn add_column_if_missing(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    column_type: &str,
) -> DbResult<()> {
    if !column_exists(conn, table_name, column_name)? {
        conn.execute(
            &format!(
                "ALTER TABLE \"{}\" ADD COLUMN {} {}",
                table_name, column_name, column_type
            ),
            [],
        )?;
        bevy::log::info!("Added column '{}' to table '{}'", column_name, table_name);
    }
    Ok(())
}

/// Get table type from global metadata
pub fn get_table_type(conn: &Connection, table_name: &str) -> DbResult<Option<String>> {
    let result = conn
        .query_row(
            "SELECT table_type FROM _Metadata WHERE table_name = ?",
            [table_name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(result)
}

/// Create the global _Metadata table
pub fn create_global_metadata_table(conn: &Connection) -> DbResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _Metadata (
            table_name TEXT PRIMARY KEY,
            table_type TEXT DEFAULT 'main',
            parent_table TEXT,
            parent_column TEXT,
            ai_allow_add_rows INTEGER DEFAULT 0,
            ai_table_context TEXT,
            ai_grounding_with_google_search INTEGER DEFAULT 0,
            ai_active_group TEXT,
            display_order INTEGER,
            category TEXT,
            hidden INTEGER DEFAULT 0,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;
    Ok(())
}

/// Create main data table
pub fn create_main_data_table(conn: &Connection, table_name: &str, column_defs: &[String]) -> DbResult<()> {
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
        table_name,
        column_defs.join(", ")
    );
    conn.execute(&create_sql, [])?;

    // Create index
    let index_name = table_name.replace(" ", "_");
    conn.execute(
        &format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_row_index ON \"{}\"(row_index)",
            index_name, table_name
        ),
        [],
    )?;

    Ok(())
}

/// Create metadata table for a sheet
pub fn create_sheet_metadata_table(conn: &Connection, meta_table: &str) -> DbResult<()> {
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                column_index INTEGER UNIQUE NOT NULL,
                column_name TEXT NOT NULL UNIQUE,
                data_type TEXT NOT NULL,
                validator_type TEXT,
                validator_config TEXT,
                ai_context TEXT,
                filter_expr TEXT,
                ai_enable_row_generation INTEGER DEFAULT 0,
                ai_include_in_send INTEGER DEFAULT 1,
                deleted INTEGER DEFAULT 0
            )",
            meta_table
        ),
        [],
    )?;
    Ok(())
}

/// Insert a single column metadata row
pub fn insert_column_metadata(
    conn: &Connection,
    meta_table: &str,
    column_index: i32,
    column_name: &str,
    data_type: &str,
    validator_type: Option<&str>,
    validator_config: Option<&str>,
    ai_context: Option<&str>,
    filter_expr: Option<&str>,
    ai_enable_row_generation: i32,
    ai_include_in_send: i32,
    deleted: i32,
) -> DbResult<()> {
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO \"{}\" 
             (column_index, column_name, data_type, validator_type, validator_config, ai_context, filter_expr, ai_enable_row_generation, ai_include_in_send, deleted)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            meta_table
        ),
        params![
            column_index,
            column_name,
            data_type,
            validator_type,
            validator_config,
            ai_context,
            filter_expr,
            ai_enable_row_generation,
            ai_include_in_send,
            deleted
        ],
    )?;
    Ok(())
}

/// Insert or ignore column metadata by column_index
pub fn insert_column_metadata_if_missing(
    conn: &Connection,
    meta_table: &str,
    column_index: i32,
    column_name: &str,
    data_type: &str,
) -> DbResult<()> {
    conn.execute(
        &format!(
            "INSERT OR IGNORE INTO \"{}\" (column_index, column_name, data_type) VALUES (?, ?, ?)",
            meta_table
        ),
        params![column_index, column_name, data_type],
    )?;
    Ok(())
}

/// Create AI groups table for a sheet
pub fn create_ai_groups_table(conn: &Connection, groups_table: &str, meta_table: &str) -> DbResult<()> {
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                column_id INTEGER NOT NULL,
                group_name TEXT NOT NULL,
                is_enabled INTEGER DEFAULT 0,
                FOREIGN KEY (column_id) REFERENCES \"{}\"(id) ON DELETE CASCADE,
                UNIQUE(column_id, group_name)
            )",
            groups_table, meta_table
        ),
        [],
    )?;
    Ok(())
}

/// Insert AI group membership
pub fn insert_ai_group_column(
    conn: &Connection,
    groups_table: &str,
    meta_table: &str,
    group_name: &str,
    column_index: i32,
) -> DbResult<()> {
    conn.execute(
        &format!(
            "INSERT OR IGNORE INTO \"{}\" (column_id, group_name, is_enabled)
             SELECT id, ?, 1 FROM \"{}\" WHERE column_index = ?",
            groups_table, meta_table
        ),
        params![group_name, column_index],
    )?;
    Ok(())
}

/// Create structure table
pub fn create_structure_data_table(
    conn: &Connection,
    structure_table: &str,
    column_defs: &[String],
) -> DbResult<()> {
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" ({})",
        structure_table,
        column_defs.join(", ")
    );
    
    bevy::log::info!("Creating structure table with SQL: {}", create_sql);
    conn.execute(&create_sql, [])?;

    // Create indexes
    let index_name = structure_table.replace(" ", "_");
    conn.execute(
        &format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_parent_key ON \"{}\"(parent_key)",
            index_name, structure_table
        ),
        [],
    )?;

    Ok(())
}

/// Drop a table if it exists
pub fn drop_table(conn: &Connection, table_name: &str) -> DbResult<()> {
    conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", table_name), [])?;
    Ok(())
}

/// Register structure table in global metadata
pub fn register_structure_table(
    conn: &Connection,
    structure_table: &str,
    parent_table: &str,
    parent_column: &str,
) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO _Metadata (table_name, table_type, parent_table, parent_column, hidden)
         VALUES (?, 'structure', ?, ?, 1)",
        params![structure_table, parent_table, parent_column],
    )?;
    Ok(())
}

/// Insert or replace table-level metadata in global _Metadata
pub fn upsert_table_metadata(
    conn: &Connection,
    table_name: &str,
    ai_allow_add_rows: i32,
    ai_table_context: Option<&str>,
    ai_active_group: Option<&str>,
    category: Option<&str>,
    display_order: Option<i32>,
    hidden: i32,
) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO _Metadata 
         (table_name, table_type, ai_allow_add_rows, ai_table_context, ai_active_group, category, display_order, hidden)
         VALUES (?, 'main', ?, ?, ?, ?, ?, ?)",
        params![
            table_name,
            ai_allow_add_rows,
            ai_table_context,
            ai_active_group,
            category,
            display_order,
            hidden
        ],
    )?;
    Ok(())
}

/// Update a specific row in a table
pub fn update_table_metadata_hidden(conn: &Connection, condition: &str) -> DbResult<()> {
    conn.execute(
        &format!("UPDATE _Metadata SET hidden = 1 WHERE {}", condition),
        [],
    )?;
    Ok(())
}
