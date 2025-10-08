// src/sheets/database/error.rs

use std::fmt;

#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    SerdeJson(serde_json::Error),
    StructureChanged(String),
    TableNotFound(String),
    InvalidMetadata(String),
    MigrationFailed(String),
    Other(String),
}

pub type DbResult<T> = Result<T, DbError>;

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Sqlite(e) => write!(f, "SQLite error: {}", e),
            DbError::Io(e) => write!(f, "I/O error: {}", e),
            DbError::SerdeJson(e) => write!(f, "JSON error: {}", e),
            DbError::StructureChanged(msg) => write!(f, "Structure changed: {}", msg),
            DbError::TableNotFound(name) => write!(f, "Table not found: {}", name),
            DbError::InvalidMetadata(msg) => write!(f, "Invalid metadata: {}", msg),
            DbError::MigrationFailed(msg) => write!(f, "Migration failed: {}", msg),
            DbError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sqlite(e)
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        DbError::Io(e)
    }
}

impl From<serde_json::Error> for DbError {
    fn from(e: serde_json::Error) -> Self {
        DbError::SerdeJson(e)
    }
}
