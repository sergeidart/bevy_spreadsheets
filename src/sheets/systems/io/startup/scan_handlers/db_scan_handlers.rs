// src/sheets/systems/io/startup/scan_handlers/db_scan_handlers.rs
//! Handlers for database scanning and loading.

use crate::sheets::events::RequestSheetRevalidation;
use crate::sheets::resources::SheetRegistry;
use crate::sheets::database::daemon_client::DaemonClient;
use bevy::prelude::*;
use std::path::Path;
use walkdir::WalkDir;

use crate::sheets::systems::io::get_default_data_base_path;

/// Scan for SQLite database files and register them as categories (lazy loading)
pub fn scan_and_load_database_files(
    registry: &mut SheetRegistry,
    _revalidate_writer: &mut EventWriter<RequestSheetRevalidation>,
    daemon_client: &DaemonClient,
) {
    let base_path = get_default_data_base_path();
    info!(
        "Startup DB Scan: Scanning directory '{:?}' for database files...",
        base_path
    );

    if !base_path.exists() {
        info!("Startup DB Scan: Data directory does not exist. Nothing to scan.");
        return;
    }

    // Find all .db files in the SkylineDB directory
    let mut db_files = Vec::new();
    for entry in WalkDir::new(&base_path)
        .max_depth(1) // Only look in root of SkylineDB, not subdirectories
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if path
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("db"))
        {
            db_files.push(path.to_path_buf());
        }
    }

    if db_files.is_empty() {
        info!("Startup DB Scan: No database files (.db) found.");
        return;
    }

    info!("Startup DB Scan: Found {} database file(s)", db_files.len());

    // Register databases as categories without loading tables (lazy loading)
    for db_path in db_files {
        register_database_as_category(registry, &db_path, daemon_client);
    }
}

/// Register a database as a category without loading tables (lazy loading optimization)
fn register_database_as_category(
    registry: &mut SheetRegistry,
    db_path: &Path,
    daemon_client: &DaemonClient,
) {
    let db_name = db_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());

    info!(
        "Startup DB Scan: Registering database '{}' as category (lazy loading)",
        db_name
    );

    // Just verify the database can be opened
    match crate::sheets::database::connection::DbConnection::open_existing(db_path) {
        Ok(mut conn) => {
            // Check if this is a SkylineDB database (has _Metadata table)
            let has_metadata_table = check_has_metadata_table(&conn);

            // If SkylineDB metadata exists, ensure it's migrated to latest schema
            if has_metadata_table {
                if let Err(e) = crate::sheets::database::schema::ensure_global_metadata_table(&conn, daemon_client) {
                    error!(
                        "Startup DB Scan: Failed to ensure _Metadata schema in '{}': {}",
                        db_name, e
                    );
                    return;
                }

                // Apply migration fixes to ensure data integrity
                info!("Startup DB Scan: Applying migration fixes to '{}'...", db_name);
                let mut fix_manager = crate::sheets::database::migration::OccasionalFixManager::new();
                fix_manager.register_fix(Box::new(
                    crate::sheets::database::migration::fix_row_index_duplicates::FixRowIndexDuplicates
                ));
                fix_manager.register_fix(Box::new(
                    crate::sheets::database::migration::parent_key_to_row_index::MigrateParentKeyToRowIndex
                ));
                fix_manager.register_fix(Box::new(
                    crate::sheets::database::migration::cleanup_temp_new_row_index::CleanupTempNewRowIndex
                ));
                fix_manager.register_fix(Box::new(
                    crate::sheets::database::migration::hide_temp_new_row_index_in_metadata::HideTempNewRowIndexInMetadata
                ));
                fix_manager.register_fix(Box::new(
                    crate::sheets::database::migration::remove_grand_parent_columns::RemoveGrandParentColumns
                ));

                match fix_manager.apply_all_fixes(&mut conn, &daemon_client) {
                    Ok(applied) => {
                        if !applied.is_empty() {
                            info!("Startup DB Scan: Applied migration fixes to '{}': {:?}", db_name, applied);
                        }
                    }
                    Err(e) => {
                        error!("Startup DB Scan: Failed to apply migration fixes to '{}': {}", db_name, e);
                    }
                }
            }

            // Just create the category, don't load tables yet
            let _ = registry.create_category(db_name.clone());
            info!(
                "Startup DB Scan: Database '{}' registered as empty category (tables will load on demand)",
                db_name
            );
        }
        Err(e) => {
            error!(
                "Startup DB Scan: Failed to open database '{}': {}",
                db_path.display(),
                e
            );
        }
    }
}

/// Load all tables from a single database file (called on-demand when category is selected)
pub fn load_database_tables(
    registry: &mut SheetRegistry,
    _revalidate_writer: &mut EventWriter<RequestSheetRevalidation>,
    db_path: &Path,
    _daemon_client: &DaemonClient,
) {
    let db_name = db_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());

    info!(
        "Lazy Load: Loading table list from database: {}",
        db_path.display()
    );

    let conn = match crate::sheets::database::connection::DbConnection::open_existing(db_path) {
        Ok(conn) => conn,
        Err(e) => {
            error!(
                "Lazy Load: Failed to open database '{}': {}",
                db_path.display(),
                e
            );
            return;
        }
    };

    // Check if this is a SkylineDB database (has _Metadata table)
    let has_metadata_table = check_has_metadata_table(&conn);

    // Get list of tables
    let table_names = get_table_names(&conn, has_metadata_table, &db_name);

    if table_names.is_empty() {
        info!(
            "Lazy Load: No tables found in '{}' (empty database)",
            db_name
        );
        return;
    }

    info!(
        "Lazy Load: Found {} table(s) in '{}'",
        table_names.len(),
        db_name
    );

    // Register table stubs (without loading data)
    for table_name in table_names {
        register_table_stub(registry, &conn, &table_name, &db_name);
    }
}

/// Check if database has SkylineDB metadata table
fn check_has_metadata_table(conn: &rusqlite::Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_Metadata'",
        [],
        |row| row.get::<_, i32>(0).map(|v| v > 0),
    )
    .unwrap_or(false)
}

/// Get list of table names from database
fn get_table_names(
    conn: &rusqlite::Connection,
    has_metadata_table: bool,
    db_name: &str,
) -> Vec<String> {
    if has_metadata_table {
        // Use SkylineDB metadata system
        match crate::sheets::database::reader::DbReader::list_sheets(conn) {
            Ok(names) => names,
            Err(e) => {
                error!(
                    "Startup DB Scan: Failed to list sheets from metadata in '{}': {}",
                    db_name, e
                );
                Vec::new()
            }
        }
    } else {
        // No metadata table - scan SQLite system tables directly
        info!(
            "Startup DB Scan: Database '{}' has no _Metadata table. Loading as generic SQLite database.",
            db_name
        );

        match conn.prepare(
            "SELECT name FROM sqlite_master 
             WHERE type='table' 
             AND name NOT LIKE 'sqlite_%'
             AND name NOT LIKE '%_Metadata'
             ORDER BY name",
        ) {
            Ok(mut stmt) => match stmt.query_map([], |row| row.get(0)) {
                Ok(rows) => rows.filter_map(Result::ok).collect(),
                Err(e) => {
                    error!("Startup DB Scan: Failed to query table names: {}", e);
                    Vec::new()
                }
            },
            Err(e) => {
                error!("Startup DB Scan: Failed to prepare query: {}", e);
                Vec::new()
            }
        }
    }
}

/// Register a table as a stub (metadata only, no data) for lazy loading
fn register_table_stub(
    registry: &mut SheetRegistry,
    conn: &rusqlite::Connection,
    table_name: &str,
    db_name: &str,
) {
    // Check if this is a structure table (child table)
    // Structure tables should be hidden by default
    let is_structure = crate::sheets::database::schema::queries::get_table_type(conn, table_name)
        .ok()
        .and_then(|t| t)
        .map(|t| t == "structure")
        .unwrap_or(false);
    
    // Also check explicit hidden flag from _Metadata
    let hidden_in_db = conn
        .query_row(
            "SELECT hidden FROM _Metadata WHERE table_name = ?",
            [table_name],
            |row| row.get::<_, Option<i32>>(0),
        )
        .ok()
        .flatten();
    
    // If hidden is explicitly set in DB, use that; otherwise default based on is_structure
    let hidden = hidden_in_db.map(|v| v != 0).unwrap_or(is_structure);
    
    // Create a minimal SheetGridData with just metadata, no actual data
    let metadata = crate::sheets::definitions::SheetMetadata {
        sheet_name: table_name.to_string(),
        data_filename: format!("{}.json", table_name), // Not used for DB-backed sheets
        category: Some(db_name.to_string()),
        columns: vec![], // Will be loaded on demand
        hidden,
        ai_schema_groups: vec![],
        ai_enable_row_generation: false,
        ai_general_rule: None,
        ai_model_id: String::from("gpt-4o-mini"),
        ai_temperature: None,
        requested_grounding_with_google_search: None,
        ai_active_schema_group: None,
        random_picker: None,
        structure_parent: None,
    };

    let sheet_data = crate::sheets::definitions::SheetGridData {
        metadata: Some(metadata),
        grid: Vec::new(), // Empty - will be loaded on demand
        row_indices: Vec::new(),
    };

    registry.add_or_replace_sheet(
        Some(db_name.to_string()),
        table_name.to_string(),
        sheet_data,
    );

    debug!(
        "Lazy Load: Registered table stub '{}' in category '{}'",
        table_name, db_name
    );
}
