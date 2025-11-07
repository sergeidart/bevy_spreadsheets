// src/sheets/database/connection.rs

use super::error::DbResult;
use rusqlite::Connection;
use std::path::Path;

pub struct DbConnection;

impl DbConnection {
    /// Creates a new database with WAL mode enabled
    pub fn create_new(path: &Path) -> DbResult<Connection> {
        let conn = Connection::open(path)?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA temp_store=MEMORY;",
        )?;

        // Create global metadata table
        super::schema::ensure_global_metadata_table(&conn)?;

        Ok(conn)
    }

    /// Opens an existing database and ensures WAL mode is enabled
    /// CRITICAL: Always use this instead of Connection::open() to ensure journaling works!
    pub fn open_existing(path: &Path) -> DbResult<Connection> {
        let conn = Connection::open(path)?;

        // MUST configure WAL mode every time we open a connection
        // SQLite PRAGMA settings are connection-specific, not database-specific
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;",
        )?;

        Ok(conn)
    }
}
