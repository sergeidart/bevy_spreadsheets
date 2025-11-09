// src/sheets/database/schema/migrations.rs

use super::super::error::DbResult;
use rusqlite::{params, Connection};

/// Create migration tracking table
pub fn ensure_migration_tracking(
    _conn: &Connection,
    daemon_client: &super::super::daemon_client::DaemonClient,
) -> DbResult<()> {
    use crate::sheets::database::daemon_client::Statement;
    
    let stmt = Statement {
        sql: "CREATE TABLE IF NOT EXISTS _SchemaVersions (
            version INTEGER PRIMARY KEY,
            applied_at TEXT DEFAULT CURRENT_TIMESTAMP,
            description TEXT
        )".to_string(),
        params: vec![],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e
        ))))?;
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
pub fn mark_migration_applied(
    _conn: &Connection,
    version: i32,
    description: &str,
    daemon_client: &super::super::daemon_client::DaemonClient,
) -> DbResult<()> {
    use crate::sheets::database::daemon_client::Statement;
    
    let stmt = Statement {
        sql: "INSERT OR IGNORE INTO _SchemaVersions (version, description) VALUES (?, ?)".to_string(),
        params: vec![
            serde_json::Value::Number(version.into()),
            serde_json::Value::String(description.to_string()),
        ],
    };
    
    daemon_client.exec_batch(vec![stmt])
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e
        ))))?;
    Ok(())
}
