// src/sheets/database/connection.rs

use super::error::DbResult;
use super::daemon_client::DaemonClient;
use rusqlite::Connection;
use std::path::Path;

pub struct DbConnection;

impl DbConnection {
    /// Creates a new database with WAL mode enabled
    pub fn create_new(path: &Path, daemon_client: &DaemonClient) -> DbResult<Connection> {
        let conn = Connection::open(path)?;

        // Set WAL mode and verify it was set
        let journal_mode: String = conn.query_row(
            "PRAGMA journal_mode=WAL",
            [],
            |row| row.get(0)
        )?;
        
        if journal_mode.to_uppercase() != "WAL" {
            bevy::log::error!(
                "Failed to set WAL mode on new database {:?}. Current mode: {}",
                path.file_name(),
                journal_mode
            );
        } else {
            bevy::log::info!("WAL mode activated for new database {:?}", path.file_name());
        }

        // Set other pragmas
        conn.execute_batch(
            "PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA temp_store=MEMORY;",
        )?;

        // Create global metadata table
        super::schema::ensure_global_metadata_table(&conn, daemon_client)?;

        Ok(conn)
    }

    /// Opens an existing database and ensures WAL mode is enabled
    /// CRITICAL: Always use this instead of Connection::open() to ensure journaling works!
    pub fn open_existing(path: &Path) -> DbResult<Connection> {
        let conn = Connection::open(path)?;

        // MUST configure WAL mode every time we open a connection
        // SQLite PRAGMA settings are connection-specific, not database-specific
        // Note: PRAGMA journal_mode=WAL returns the mode that was set
        let journal_mode: String = conn.query_row(
            "PRAGMA journal_mode=WAL",
            [],
            |row| row.get(0)
        )?;
        
        // Verify WAL mode was actually set
        if journal_mode.to_uppercase() != "WAL" {
            bevy::log::warn!(
                "Failed to set WAL mode on database {:?}. Current mode: {}. This may indicate the database is in use by another connection.",
                path.file_name(),
                journal_mode
            );
        } else {
            bevy::log::debug!("WAL mode activated for database {:?}", path.file_name());
        }

        // Set other pragmas
        conn.execute_batch(
            "PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;",
        )?;

        Ok(conn)
    }
}
