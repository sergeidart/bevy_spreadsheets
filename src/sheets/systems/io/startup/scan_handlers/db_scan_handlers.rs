// src/sheets/systems/io/startup/scan_handlers/db_scan_handlers.rs
//! Handlers for database scanning and loading.

use crate::sheets::events::RequestSheetRevalidation;
use crate::sheets::resources::SheetRegistry;
use bevy::prelude::*;
use std::path::Path;
use walkdir::WalkDir;

use super::schema_handlers::infer_schema_and_load_table;
use crate::sheets::systems::io::get_default_data_base_path;

/// Scan for SQLite database files and load tables as sheets
pub fn scan_and_load_database_files(
    registry: &mut SheetRegistry,
    revalidate_writer: &mut EventWriter<RequestSheetRevalidation>,
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

    // Load tables from each database
    for db_path in db_files {
        load_database_tables(registry, revalidate_writer, &db_path);
    }
}

/// Load all tables from a single database file
fn load_database_tables(
    registry: &mut SheetRegistry,
    revalidate_writer: &mut EventWriter<RequestSheetRevalidation>,
    db_path: &Path,
) {
    let db_name = db_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());

    info!(
        "Startup DB Scan: Loading tables from database: {}",
        db_path.display()
    );

    let conn = match rusqlite::Connection::open(db_path) {
        Ok(conn) => conn,
        Err(e) => {
            error!(
                "Startup DB Scan: Failed to open database '{}': {}",
                db_path.display(),
                e
            );
            return;
        }
    };

    // Configure connection for better concurrency
    if let Err(e) = conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;",
    ) {
        warn!("Startup DB Scan: Failed to configure database: {}", e);
    }

    // Check if this is a SkylineDB database (has _Metadata table)
    let has_metadata_table = check_has_metadata_table(&conn);

    // If SkylineDB metadata exists, ensure it's migrated to latest schema
    if has_metadata_table {
        if let Err(e) = crate::sheets::database::schema::ensure_global_metadata_table(&conn) {
            error!(
                "Startup DB Scan: Failed to ensure _Metadata schema in '{}': {}",
                db_name, e
            );
        }
    }

    // Get list of tables
    let table_names = get_table_names(&conn, has_metadata_table, &db_name);

    // Always create the category (database) even if empty
    let _ = registry.create_category(db_name.clone());

    if table_names.is_empty() {
        info!(
            "Startup DB Scan: No tables found in '{}' (empty database)",
            db_name
        );
        return;
    }

    info!(
        "Startup DB Scan: Found {} table(s) in '{}'",
        table_names.len(),
        db_name
    );

    // Load each table
    for table_name in table_names {
        load_single_table(
            registry,
            revalidate_writer,
            &conn,
            &table_name,
            &db_name,
            has_metadata_table,
        );
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

/// Load a single table from the database
fn load_single_table(
    registry: &mut SheetRegistry,
    revalidate_writer: &mut EventWriter<RequestSheetRevalidation>,
    conn: &rusqlite::Connection,
    table_name: &str,
    db_name: &str,
    has_metadata_table: bool,
) {
    // Try to load with metadata first
    let (metadata, grid, row_indices) = if has_metadata_table {
        // Load from SkylineDB metadata
        match crate::sheets::database::reader::DbReader::read_metadata(conn, table_name) {
            Ok(mut metadata) => {
                // Load grid data
                match crate::sheets::database::reader::DbReader::read_grid_data(
                    conn,
                    table_name,
                    &metadata,
                ) {
                    Ok((grid, row_indices)) => (metadata, grid, row_indices),
                    Err(e) => {
                        error!(
                            "Startup DB Scan: Failed to load grid data for '{}': {}",
                            table_name, e
                        );
                        return;
                    }
                }
            }
            Err(e) => {
                error!(
                    "Startup DB Scan: Failed to load metadata for '{}': {}",
                    table_name, e
                );
                return;
            }
        }
    } else {
        // No metadata - infer schema from SQLite table structure
        info!(
            "Startup DB Scan: Inferring schema for table '{}' from SQLite structure",
            table_name
        );

        match infer_schema_and_load_table(conn, table_name, db_name) {
            Ok((metadata, grid)) => (metadata, grid, Vec::new()), // Inferred schemas don't have row_indices yet
            Err(e) => {
                error!(
                    "Startup DB Scan: Failed to infer schema for '{}': {}",
                    table_name, e
                );
                return;
            }
        }
    };

    // Register the sheet in the registry
    let sheet_data = crate::sheets::definitions::SheetGridData {
        metadata: Some(metadata.clone()),
        grid,
        row_indices,
    };
    
    info!(
        "Startup DB Scan: Registering table '{}' with {} rows and {} row_indices",
        table_name, sheet_data.grid.len(), sheet_data.row_indices.len()
    );

    registry.add_or_replace_sheet(Some(db_name.to_string()), table_name.to_string(), sheet_data);

    info!(
        "Startup DB Scan: Successfully loaded table '{}' from category '{}'",
        table_name, db_name
    );

    // Trigger render cache build for the newly loaded sheet
    revalidate_writer.write(RequestSheetRevalidation {
        category: Some(db_name.to_string()),
        sheet_name: table_name.to_string(),
    });
}
