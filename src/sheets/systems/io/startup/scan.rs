// src/sheets/systems/io/startup/scan.rs
use crate::sheets::{
    definitions::{SheetMetadata, ColumnDefinition, ColumnDataType, ColumnValidator},
    events::RequestSheetRevalidation,
    resources::SheetRegistry,
    systems::io::{
        get_default_data_base_path,
        save::save_single_sheet,
        startup::{grid_load, metadata_load, registration},
        validator,
    },
};
use bevy::prelude::*;
use walkdir::WalkDir;

pub fn scan_filesystem_for_unregistered_sheets(
    mut registry: ResMut<SheetRegistry>,
    // ADDED revalidation writer
    mut revalidate_writer: EventWriter<RequestSheetRevalidation>,
) {
    let base_path = get_default_data_base_path();
    info!(
        "Startup Scan: Recursively scanning directory '{:?}' for sheets...",
        base_path
    );

    if !base_path.exists() {
        info!("Startup Scan: Data directory does not exist. Nothing to scan.");
        return;
    }

    let mut found_unregistered_count = 0;
    let mut potential_grid_files = Vec::new();
    // ADDED: Track successfully registered sheets for validation
    let mut sheets_registered_in_scan = Vec::new();

    // --- Also collect empty category directories so they appear even without sheets ---
    let mut empty_dirs: Vec<String> = Vec::new();
    for entry_result in WalkDir::new(&base_path)
        .min_depth(1)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
    {
        let entry = entry_result;
        if entry.depth() == 1 && entry.file_type().is_dir() {
            let dir_path = entry.path();
            // consider it a category dir if it has no files with .json inside
            let mut has_any_json = false;
            if let Ok(mut rd) = std::fs::read_dir(dir_path) {
                while let Some(Ok(child)) = rd.next() {
                    if child.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        if child
                            .path()
                            .extension()
                            .map_or(false, |ext| ext.eq_ignore_ascii_case("json"))
                        {
                            has_any_json = true;
                            break;
                        }
                    }
                }
            }
            if !has_any_json {
                if let Some(name_os) = dir_path.file_name() {
                    let name = name_os.to_string_lossy().to_string();
                    // Registrar: push if valid category name and not already implicitly present
                    if registry
                        .get_sheet_names_in_category(&Some(name.clone()))
                        .is_empty()
                    {
                        empty_dirs.push(name);
                    }
                }
            }
        }
    }

    // Register explicit empty categories so they show up
    for cat_name in empty_dirs {
        // Use registry API which avoids duplicates
        let _ = registry.create_category(cat_name);
    }

    // --- (Finding potential grid files remains the same) ---
    for entry_result in WalkDir::new(&base_path)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry_result.path();
        let is_json = path
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("json"));
        let is_meta_file = path
            .file_name()
            .map_or(false, |name| name.to_string_lossy().ends_with(".meta.json"));
        if is_json && !is_meta_file {
            potential_grid_files.push(path.to_path_buf());
        }
    }
    if potential_grid_files.is_empty() {
        info!("Startup Scan: No potential unregistered grid files (.json) found.");
        return;
    } else {
        trace!("Found potential grid files: {:?}", potential_grid_files);
    }

    for grid_path in potential_grid_files {
        // --- (Deriving name, category, validation remains the same) ---
        let filename = grid_path.file_name().map_or_else(
            || "unknown.json".to_string(),
            |os| os.to_string_lossy().into_owned(),
        );
        let sheet_name_candidate = grid_path.file_stem().map_or_else(
            || {
                filename
                    .trim_end_matches(".json")
                    .trim_end_matches(".JSON")
                    .to_string()
            },
            |os| os.to_string_lossy().into_owned(),
        );
        let relative_path = match grid_path.strip_prefix(&base_path) {
            Ok(rel) => rel,
            Err(_) => {
                error!(
                    "Failed to strip base path from '{}'. Skipping.",
                    grid_path.display()
                );
                continue;
            }
        };
        let category: Option<String> = relative_path
            .parent()
            .and_then(|p| p.file_name())
            .map(|os_str| os_str.to_string_lossy().into_owned())
            .filter(|dir_name| !dir_name.is_empty());
        trace!(
            "Processing '{}'. Derived Name: '{}'. Derived Category: '{:?}'",
            grid_path.display(),
            sheet_name_candidate,
            category
        );
        if let Err(e) = validator::validate_derived_sheet_name(&sheet_name_candidate) {
            warn!(
                "Skipping file '{}': Validation failed: {}",
                grid_path.display(),
                e
            );
            continue;
        }
        if let Some(cat_name) = category.as_deref() {
            if let Err(e) = validator::validate_derived_category_name(cat_name) {
                warn!(
                    "Skipping file '{}' in category '{:?}': Validation failed: {}",
                    grid_path.display(),
                    category,
                    e
                );
                continue;
            }
        }
        let already_registered_name = registry.does_sheet_exist(&sheet_name_candidate);
        let already_registered_file =
            registry
                .get_sheet(&category, &sheet_name_candidate)
                .map_or(false, |data| {
                    data.metadata
                        .as_ref()
                        .map_or(false, |m| m.data_filename == filename)
                });
        if already_registered_name && !already_registered_file {
            warn!("Skipping file '{}' (Sheet name '{}' already exists, possibly in another category or with a different filename).", grid_path.display(), sheet_name_candidate);
            continue;
        }
        if already_registered_file {
            trace!(
                "Skipping file '{}': Sheet '{}' in category '{:?}' seems already registered.",
                grid_path.display(),
                sheet_name_candidate,
                category
            );
            continue;
        }
        trace!("Found potential unregistered grid file: '{}'. Attempting load as sheet '{}' in category '{:?}'.", filename, sheet_name_candidate, category );

        // --- (Metadata and Grid Loading remains the same) ---
        let mut needs_metadata_save = false;
        let meta_load_result = metadata_load::load_and_validate_metadata_file(
            &base_path,
            relative_path,
            &sheet_name_candidate,
            &category,
            &grid_path,
        );
        let mut loaded_metadata = match meta_load_result {
            Ok((meta_opt, corrected)) => {
                if corrected {
                    needs_metadata_save = true;
                }
                meta_opt
            }
            Err(e) => {
                error!("Metadata loading failed unexpectedly for sheet '{}': {}. Will generate default.", sheet_name_candidate, e);
                None
            }
        };
        let grid_load_result = grid_load::load_grid_data_file(&grid_path);
        let final_grid: Vec<Vec<String>>;
        match grid_load_result {
            Ok(Some(grid)) => {
                final_grid = grid;
            }
            Ok(None) => {
                info!(
                    "Grid file '{}' for '{}' is empty. Using empty grid.",
                    grid_path.display(),
                    sheet_name_candidate
                );
                final_grid = Vec::new();
            }
            Err(e) => {
                error!(
                    "Failed to load grid data from '{}' for sheet '{}': {}",
                    grid_path.display(),
                    sheet_name_candidate,
                    e
                );
                continue;
            }
        }
        let final_metadata = loaded_metadata.take().unwrap_or_else(|| {
            info!(
                "Generating default metadata for sheet '{}' category '{:?}'.",
                sheet_name_candidate, category
            );
            needs_metadata_save = true;
            let num_cols = final_grid.first().map_or(0, |r| r.len());
            SheetMetadata::create_generic(
                sheet_name_candidate.clone(),
                filename.clone(),
                num_cols,
                category.clone(),
            )
        });

        // --- Registration ---
        if registration::add_scanned_sheet_to_registry(
            &mut registry,
            category.clone(),
            sheet_name_candidate.clone(),
            final_metadata.clone(), // Clone for registration
            final_grid,             // Pass ownership of grid
            grid_path.display().to_string(),
        ) {
            // If registration was successful
            found_unregistered_count += 1;
            // ADDED: Track for validation
            sheets_registered_in_scan.push((category.clone(), sheet_name_candidate.clone()));

            // --- (Save Corrected/Generated Metadata remains the same) ---
            if needs_metadata_save {
                let registry_immut = registry.as_ref();
                trace!(
                    "Saving corrected/generated metadata for '{:?}/{}'",
                    category,
                    sheet_name_candidate
                );
                save_single_sheet(registry_immut, &final_metadata);
            }
        }
    } // End processing loop

    if found_unregistered_count > 0 {
        info!(
            "Startup Scan: Found and processed {} unregistered sheets.",
            found_unregistered_count
        );
        // ADDED: Trigger validation for sheets registered during scan
        for (cat, name) in sheets_registered_in_scan {
            revalidate_writer.write(RequestSheetRevalidation {
                category: cat,
                sheet_name: name,
            });
        }
    } else {
        info!("Startup Scan: No new unregistered sheets found to process.");
    }
}

/// Scan for SQLite database files and load tables as sheets
pub fn scan_and_load_database_files(
    mut registry: ResMut<SheetRegistry>,
    mut revalidate_writer: EventWriter<RequestSheetRevalidation>,
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
        if path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("db")) {
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
        let db_name = db_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unknown".to_string());

        info!(
            "Startup DB Scan: Loading tables from database: {}",
            db_path.display()
        );

        match rusqlite::Connection::open(&db_path) {
            Ok(conn) => {
                // Check if this is a SkylineDB database (has _Metadata table)
                let has_metadata_table: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_Metadata'",
                        [],
                        |row| row.get::<_, i32>(0).map(|v| v > 0),
                    )
                    .unwrap_or(false);

                // If SkylineDB metadata exists, ensure it's migrated to latest schema (adds 'hidden' if missing)
                if has_metadata_table {
                    if let Err(e) = crate::sheets::database::schema::ensure_global_metadata_table(&conn) {
                        error!("Startup DB Scan: Failed to ensure _Metadata schema in '{}': {}", db_name, e);
                    }
                }

                // Get list of tables
                let table_names: Vec<String> = if has_metadata_table {
                    // Use SkylineDB metadata system
                    match crate::sheets::database::reader::DbReader::list_sheets(&conn) {
                        Ok(names) => names,
                        Err(e) => {
                            error!(
                                "Startup DB Scan: Failed to list tables in '{}': {}",
                                db_name, e
                            );
                            continue;
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
                         ORDER BY name"
                    ) {
                        Ok(mut stmt) => {
                            match stmt.query_map([], |row| row.get(0)) {
                                Ok(rows) => {
                                    rows.collect::<Result<Vec<String>, _>>().unwrap_or_default()
                                }
                                Err(e) => {
                                    error!("Startup DB Scan: Failed to query tables: {}", e);
                                    continue;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Startup DB Scan: Failed to prepare query: {}", e);
                            continue;
                        }
                    }
                };

                // Always create the category (database) even if empty
                let _ = registry.create_category(db_name.clone());
                
                if table_names.is_empty() {
                    info!("Startup DB Scan: No tables found in '{}' (empty database)", db_name);
                    continue;
                }

                info!(
                    "Startup DB Scan: Found {} table(s) in '{}'",
                    table_names.len(),
                    db_name
                );

                for table_name in table_names {
                    // Try to load with metadata first
                    let (metadata, grid) = if has_metadata_table {
                        // Load from SkylineDB metadata
                        match crate::sheets::database::reader::DbReader::read_metadata(
                            &conn,
                            &table_name,
                        ) {
                            Ok(mut metadata) => {
                                metadata.category = Some(db_name.clone());
                                match crate::sheets::database::reader::DbReader::read_grid_data(
                                    &conn,
                                    &table_name,
                                    &metadata,
                                ) {
                                    Ok(grid) => (metadata, grid),
                                    Err(e) => {
                                        error!(
                                            "Startup DB Scan: Failed to read grid data for table '{}': {}",
                                            table_name, e
                                        );
                                        continue;
                                    }
                                }
                            }
                            Err(e) => {
                                error!(
                                    "Startup DB Scan: Failed to read metadata for table '{}': {}",
                                    table_name, e
                                );
                                continue;
                            }
                        }
                    } else {
                        // No metadata - infer schema from SQLite table structure
                        info!(
                            "Startup DB Scan: Inferring schema for table '{}' from SQLite structure",
                            table_name
                        );
                        
                        match infer_schema_and_load_table(&conn, &table_name, &db_name) {
                            Ok((meta, grid)) => (meta, grid),
                            Err(e) => {
                                error!(
                                    "Startup DB Scan: Failed to infer schema for table '{}': {}",
                                    table_name, e
                                );
                                continue;
                            }
                        }
                    };

                    // Register the sheet in the registry
                    let sheet_data = crate::sheets::definitions::SheetGridData {
                        metadata: Some(metadata.clone()),
                        grid,
                    };

                    registry.add_or_replace_sheet(
                        Some(db_name.clone()),
                        table_name.clone(),
                        sheet_data,
                    );

                    info!(
                        "Startup DB Scan: Successfully loaded table '{}' from category '{}'",
                        table_name, db_name
                    );

                    // Trigger render cache build for the newly loaded sheet
                    revalidate_writer.write(RequestSheetRevalidation {
                        category: Some(db_name.clone()),
                        sheet_name: table_name.clone(),
                    });
                }
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
}

/// Infer schema from SQLite table structure and load data
/// Used for databases without SkylineDB metadata
fn infer_schema_and_load_table(
        conn: &rusqlite::Connection,
        table_name: &str,
        db_name: &str,
    ) -> Result<(SheetMetadata, Vec<Vec<String>>), String> {
        // Get column info from SQLite
        let pragma_query = format!("PRAGMA table_info(\"{}\")", table_name);
        let mut stmt = conn
            .prepare(&pragma_query)
            .map_err(|e| format!("Failed to get table info: {}", e))?;

        let column_info: Vec<(String, String)> = stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let type_str: String = row.get(2)?;
                Ok((name, type_str))
            })
            .map_err(|e| format!("Failed to query table info: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect column info: {}", e))?;

        if column_info.is_empty() {
            return Err("No columns found in table".to_string());
        }

        // Filter out internal columns (row_index, etc.)
        let columns: Vec<ColumnDefinition> = column_info
            .iter()
            .filter(|(name, _)| name != "row_index")
            .map(|(name, sqlite_type)| {
                // Map SQLite types to our data types
                let data_type = match sqlite_type.to_uppercase().as_str() {
                    t if t.contains("INT") => ColumnDataType::I64,
                    t if t.contains("REAL") || t.contains("FLOAT") || t.contains("DOUBLE") => {
                        ColumnDataType::F64
                    }
                    t if t.contains("BOOL") => ColumnDataType::Bool,
                    _ => ColumnDataType::String,
                };

                ColumnDefinition {
                    header: name.clone(),
                    validator: Some(ColumnValidator::Basic(data_type)),
                    data_type,
                    filter: None,
                    ai_context: None,
                    ai_enable_row_generation: None,
                    ai_include_in_send: None,
                    width: None,
                    structure_schema: None,
                    structure_column_order: None,
                    structure_key_parent_column_index: None,
                    structure_ancestor_key_parent_column_indices: None,
                }
            })
            .collect();

        // Create generic metadata
        let metadata = SheetMetadata {
            sheet_name: table_name.to_string(),
            category: Some(db_name.to_string()),
            data_filename: format!("{}.json", table_name),
            columns: columns.clone(),
            ai_general_rule: None,
            ai_model_id: "gemini-flash-latest".to_string(),
            ai_temperature: None,
            ai_top_k: None,
            ai_top_p: None,
            requested_grounding_with_google_search: Some(false),
            ai_enable_row_generation: false,
            ai_schema_groups: Vec::new(),
            ai_active_schema_group: None,
            random_picker: None,
            structure_parent: None,
            hidden: false,
        };

        // Load grid data
        let column_names: Vec<String> = columns.iter().map(|c| c.header.clone()).collect();
        let select_cols = column_names
            .iter()
            .map(|name| format!("\"{}\"", name))
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!("SELECT {} FROM \"{}\"", select_cols, table_name);
        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| format!("Failed to prepare SELECT query: {}", e))?;

        let col_count = column_names.len();
        let rows = stmt
            .query_map([], |row| {
                let mut cells = Vec::new();
                for i in 0..col_count {
                    // Try to get as string, fallback to empty
                    let value: Option<String> = row.get(i).ok();
                    cells.push(value.unwrap_or_default());
                }
                Ok(cells)
            })
            .map_err(|e| format!("Failed to query rows: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect rows: {}", e))?;

        info!(
            "Infer Schema: Table '{}': loaded {} rows with {} columns",
            table_name,
            rows.len(),
            col_count
        );

        Ok((metadata, rows))
    }
