// src/sheets/database/mod.rs

pub mod checkpoint;
pub mod connection;
pub mod daemon_client;
pub mod daemon_connection;
pub mod daemon_manager;
pub mod daemon_protocol;
pub mod daemon_resource;
pub mod error;
pub mod migration;
pub mod reader;
pub mod schema;
pub mod systems;
pub mod writer;
pub mod validation;
pub use migration::MigrationTools;
pub use systems::{
    handle_export_requests, handle_migration_completion, handle_migration_requests,
    handle_upload_json_to_current_db, 
};
use rusqlite::OptionalExtension;

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
        let config = Self {
            skyline_path: Self::default_path(),
        };
        // Ensure the directory exists on creation
        let _ = config.ensure_directories();
        config
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

/// Try to open (or create) a DB file for a category name. Creates directories if needed.
pub fn open_or_create_db_for_category(category: &str) -> Result<rusqlite::Connection, String> {
    use std::fs;
    let mut base = DbConfig::default_path();
    // Ensure base directory exists
    if let Err(e) = fs::create_dir_all(&base) {
        return Err(format!("Failed to create base DB dir {:?}: {}", base, e));
    }
    base.push(format!("{}.db", category));
    // Emit a log so the exact DB path is visible when debugging SQL calls
    bevy::log::info!("Opening/creating DB file {}", base.display());
    
    // Use the proper connection method that ensures WAL mode is enabled
    match connection::DbConnection::open_existing(&base) {
        Ok(conn) => Ok(conn),
        Err(e) => Err(format!("Failed to open/create DB '{}': {}", base.display(), e)),
    }
}

/// Convenience helper to persist column-level metadata: filter, ai_context, include flag.
pub fn persist_column_metadata(
    category: &str,
    table_name: &str,
    column_index: usize,
    filter_expr: Option<&str>,
    ai_context: Option<&str>,
    ai_include: Option<bool>,
    daemon_client: &daemon_client::DaemonClient,
) -> Result<(), String> {
    let db_filename = format!("{}.db", category);
    match open_or_create_db_for_category(category) {
        Ok(conn) => {
            let _ = crate::sheets::database::schema::ensure_global_metadata_table(&conn, daemon_client)
                .map_err(|e| e.to_string())?;
            crate::sheets::database::writer::DbWriter::update_column_metadata(
                &conn,
                table_name,
                column_index,
                filter_expr,
                ai_context,
                ai_include,
                Some(&db_filename),
                daemon_client,
            )
            .map_err(|e| e.to_string())
        }
        Err(e) => Err(e),
    }
}

/// Persist validator/data_type change by column name (safe when caller index may refer to UI including technical columns)
pub fn persist_column_validator_by_name(
    category: &str,
    table_name: &str,
    column_name: &str,
    data_type: crate::sheets::definitions::ColumnDataType,
    validator: &Option<crate::sheets::definitions::ColumnValidator>,
    ai_include_in_send: Option<bool>,
    ai_enable_row_generation: Option<bool>,
    daemon_client: &daemon_client::DaemonClient,
) -> Result<(), String> {
    match open_or_create_db_for_category(category) {
        Ok(conn) => {
            let _ = crate::sheets::database::schema::ensure_global_metadata_table(&conn, daemon_client)
                .map_err(|e| e.to_string())?;
            
            // Lookup column_index by name in the metadata table
            let meta_table = format!("{}_Metadata", table_name);
            let persisted_idx: Option<i32> = conn
                .query_row(
                    &format!("SELECT column_index FROM \"{}\" WHERE column_name = ?", meta_table),
                    [column_name],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            if let Some(persisted_ci) = persisted_idx {
                // Convert persisted index to runtime index
                // Determine if this is a structure table
                let table_type: Option<String> = conn
                    .query_row(
                        "SELECT table_type FROM _Metadata WHERE table_name = ?",
                        [table_name],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                
                let is_structure = matches!(table_type.as_deref(), Some("structure"));
                
                // Calculate runtime index by adding back the technical columns
                let runtime_idx = if is_structure {
                    // Structure tables have 2 technical columns (row_index, parent_key) before persisted columns
                    persisted_ci as usize + 2
                } else {
                    // Regular tables have 1 technical column (row_index) before persisted columns
                    persisted_ci as usize + 1
                };
                
                // Get the database filename for daemon operations
                // conn.path() returns the full path, we need just the filename
                let db_filename = conn.path()
                    .and_then(|p| std::path::Path::new(p).file_name())
                    .and_then(|n| n.to_str());
                
                bevy::log::info!("🔧 persist_column_validator_by_name: db_filename={:?}", db_filename);
                
                crate::sheets::database::writer::DbWriter::update_column_validator(
                    &conn,
                    table_name,
                    runtime_idx,
                    data_type,
                    validator,
                    ai_include_in_send,
                    ai_enable_row_generation,
                    db_filename,
                    daemon_client,
                )
                .map_err(|e| e.to_string())
            } else {
                Err(format!(
                    "Column '{}' not found in {} metadata",
                    column_name, meta_table
                ))
            }
        }
        Err(e) => Err(e),
    }
}
