// src/sheets/database/mod.rs

pub mod connection;
pub mod error;
pub mod migration;
pub mod reader;
pub mod schema;
pub mod systems;
pub mod writer;

pub use connection::DbConnection;
pub use error::{DbError, DbResult};
pub use migration::{MigrationReport, MigrationTools};
pub use reader::DbReader;
pub use systems::{
    handle_export_requests, handle_migration_completion, handle_migration_requests,
    handle_upload_json_to_current_db,
};
pub use writer::DbWriter;
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
    match rusqlite::Connection::open(&base) {
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
) -> Result<(), String> {
    match open_or_create_db_for_category(category) {
        Ok(conn) => {
            let _ = crate::sheets::database::schema::ensure_global_metadata_table(&conn)
                .map_err(|e| e.to_string())?;
            crate::sheets::database::writer::DbWriter::update_column_metadata(
                &conn,
                table_name,
                column_index,
                filter_expr,
                ai_context,
                ai_include,
            )
            .map_err(|e| e.to_string())
        }
        Err(e) => Err(e),
    }
}

/// Convenience helper to persist validator/data_type changes for a column.
pub fn persist_column_validator(
    category: &str,
    table_name: &str,
    column_index: usize,
    data_type: crate::sheets::definitions::ColumnDataType,
    validator: &Option<crate::sheets::definitions::ColumnValidator>,
    ai_include_in_send: Option<bool>,
    ai_enable_row_generation: Option<bool>,
) -> Result<(), String> {
    match open_or_create_db_for_category(category) {
        Ok(conn) => {
            let _ = crate::sheets::database::schema::ensure_global_metadata_table(&conn)
                .map_err(|e| e.to_string())?;
            crate::sheets::database::writer::DbWriter::update_column_validator(
                &conn,
                table_name,
                column_index,
                data_type,
                validator,
                ai_include_in_send,
                ai_enable_row_generation,
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
) -> Result<(), String> {
    match open_or_create_db_for_category(category) {
        Ok(conn) => {
            let _ = crate::sheets::database::schema::ensure_global_metadata_table(&conn)
                .map_err(|e| e.to_string())?;
            // Lookup column_index by name in the metadata table
            let meta_table = format!("{}_Metadata", table_name);
            let idx_row: Option<i32> = conn
                .query_row(
                    &format!("SELECT column_index FROM \"{}\" WHERE column_name = ?", meta_table),
                    [column_name],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            if let Some(ci) = idx_row {
                crate::sheets::database::writer::DbWriter::update_column_validator(
                    &conn,
                    table_name,
                    ci as usize,
                    data_type,
                    validator,
                    ai_include_in_send,
                    ai_enable_row_generation,
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
