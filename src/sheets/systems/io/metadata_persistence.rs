// src/sheets/systems/io/metadata_persistence.rs
use bevy::prelude::*;
use crate::sheets::definitions::SheetMetadata;
use crate::sheets::resources::SheetRegistry;

/// Save sheet metadata to persistent storage
/// If the sheet belongs to a database category, updates SQLite _Metadata table
/// Otherwise, saves to JSON file
pub fn save_sheet_metadata(
    registry: &SheetRegistry,
    metadata: &SheetMetadata,
    category: Option<String>,
    daemon_client: &crate::sheets::database::daemon_client::DaemonClient,
) {
    if let Some(ref cat_name) = category {
        // Database-backed category: category name equals the database file stem
        let base_path = super::get_default_data_base_path();
        let db_path = base_path.join(format!("{}.db", cat_name));
        
        if db_path.exists() {
            match rusqlite::Connection::open(&db_path) {
                Ok(conn) => {
                    // Ensure metadata table has 'hidden' column (migrate if needed)
                    if let Err(e) = crate::sheets::database::schema::ensure_global_metadata_table(&conn, daemon_client) {
                        error!(
                            "Failed to ensure _Metadata schema in DB '{}': {}",
                            db_path.display(),
                            e
                        );
                    }
                    
                    if let Err(e) = crate::sheets::database::writer::DbWriter::update_table_hidden(
                        &conn,
                        &metadata.sheet_name,
                        metadata.hidden,
                        daemon_client,
                    ) {
                        error!(
                            "Failed to update hidden flag in DB for '{}': {}",
                            &metadata.sheet_name, e
                        );
                    }
                }
                Err(e) => error!("Failed to open database '{}': {}", db_path.display(), e),
            }
        } else {
            // Fallback to JSON metadata save if DB file not found
            super::save::save_single_sheet(registry, metadata);
        }
    } else {
        // JSON-backed root category
        super::save::save_single_sheet(registry, metadata);
    }
}
