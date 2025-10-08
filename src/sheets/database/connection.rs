// src/sheets/database/connection.rs

use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use super::error::{DbResult, DbError};

pub struct DbConnection {
    path: PathBuf,
    conn: Option<Connection>,
}

impl DbConnection {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            conn: None,
        }
    }
    
    pub fn open_read_only(&mut self) -> DbResult<()> {
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        
        self.conn = Some(conn);
        Ok(())
    }
    
    pub fn open_read_write(&mut self) -> DbResult<()> {
        let conn = Connection::open(&self.path)?;
        
        // Configure for better performance and concurrency
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA temp_store=MEMORY;
             PRAGMA cache_size=-64000;"
        )?;
        
        self.conn = Some(conn);
        Ok(())
    }
    
    pub fn create_new(path: &Path) -> DbResult<Connection> {
        let conn = Connection::open(path)?;
        
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA temp_store=MEMORY;"
        )?;
        
        // Create global metadata table
        super::schema::ensure_global_metadata_table(&conn)?;
        
        Ok(conn)
    }
    
    pub fn connection(&self) -> DbResult<&Connection> {
        self.conn.as_ref().ok_or_else(|| DbError::Other("Connection not open".into()))
    }
    
    pub fn connection_mut(&mut self) -> DbResult<&mut Connection> {
        self.conn.as_mut().ok_or_else(|| DbError::Other("Connection not open".into()))
    }
    
    pub fn close(&mut self) {
        self.conn = None;
    }
    
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    /// List all tables in the database (excluding metadata tables)
    pub fn list_tables(&self) -> DbResult<Vec<String>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master 
             WHERE type='table' AND name NOT LIKE '%_Metadata%' AND name != '_Metadata'
             ORDER BY name"
        )?;
        
        let tables = stmt.query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        
        Ok(tables)
    }
}
