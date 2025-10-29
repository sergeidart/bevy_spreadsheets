// src/sheets/database/schema/migrations.rs

use super::super::error::DbResult;
use rusqlite::{params, Connection};

/// Create migration tracking table
pub fn ensure_migration_tracking(conn: &Connection) -> DbResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _SchemaVersions (
            version INTEGER PRIMARY KEY,
            applied_at TEXT DEFAULT CURRENT_TIMESTAMP,
            description TEXT
        )",
        [],
    )?;
    Ok(())
}

/// Check if a specific migration version has been applied
pub fn is_migration_applied(conn: &Connection, version: i32) -> DbResult<bool> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM _SchemaVersions WHERE version = ?",
        params![version],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Mark a migration as applied
pub fn mark_migration_applied(conn: &Connection, version: i32, description: &str) -> DbResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO _SchemaVersions (version, description) VALUES (?, ?)",
        params![version, description],
    )?;
    Ok(())
}
