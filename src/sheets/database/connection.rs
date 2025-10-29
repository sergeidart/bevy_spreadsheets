// src/sheets/database/connection.rs

use super::error::DbResult;
use rusqlite::Connection;
use std::path::Path;

pub struct DbConnection;

impl DbConnection {
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
}
