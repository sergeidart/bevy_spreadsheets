// src/sheets/database/mod.rs

pub mod schema;
pub mod connection;
pub mod reader;
pub mod writer;
pub mod migration;
pub mod error;
pub mod systems;

pub use connection::DbConnection;
pub use reader::DbReader;
pub use writer::DbWriter;
pub use migration::{MigrationTools, MigrationReport};
pub use error::{DbError, DbResult};
pub use systems::{handle_migration_requests, handle_upload_json_to_current_db, handle_export_requests, handle_migration_completion};

use std::path::PathBuf;

/// Database storage configuration
#[derive(Debug, Clone)]
pub struct DbConfig {
    pub skyline_path: PathBuf,
}

impl DbConfig {
    pub fn default_path() -> PathBuf {
        let documents = directories_next::UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        documents.join("SkylineDB")
    }
    
    pub fn new() -> Self {
        Self {
            skyline_path: Self::default_path(),
        }
    }
    
    pub fn ensure_directories(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.skyline_path)?;
        Ok(())
    }
}

impl Default for DbConfig {
    fn default() -> Self {
        Self::new()
    }
}
