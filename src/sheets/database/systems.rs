// src/sheets/database/systems.rs

use bevy::prelude::*;
use crate::sheets::events::{RequestMigrateJsonToDb, RequestUploadJsonToCurrentDb, MigrationCompleted, MigrationProgress, SheetOperationFeedback, RequestExportSheetToJson};
use super::migration::MigrationTools;
use crate::sheets::systems::io::get_default_data_base_path;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Resource, Default)]
pub struct MigrationBackgroundState {
    pub progress_rx: Option<Arc<Mutex<Receiver<MigrationProgress>>>>, // progress updates
    pub completion_rx: Option<Arc<Mutex<Receiver<Result<(super::migration::MigrationReport, PathBuf), String>>>>>, // final result with db path
    /// Optional target to auto-select after completion: (category/db name, table name)
    pub post_select: Option<(String, String)>,
}

pub fn handle_migration_requests(
    mut events: EventReader<RequestMigrateJsonToDb>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut bg_state: ResMut<MigrationBackgroundState>,
) {
    for event in events.read() {
        info!("Starting migration from {:?} to {:?}", event.json_folder_path, event.target_db_path);
        // Spawn background thread and set up channels
        let (tx_prog, rx_prog) = channel::<MigrationProgress>();
        let (tx_done, rx_done) = channel::<Result<(super::migration::MigrationReport, PathBuf), String>>();

        let json_folder = event.json_folder_path.clone();
        let db_path = event.target_db_path.clone();
        let create_new = event.create_new_db;

        // Send initial info message
        feedback_writer.write(SheetOperationFeedback { message: "Migration started...".into(), is_error: false });

    thread::spawn(move || {
            let run = || -> Result<(super::migration::MigrationReport, PathBuf), String> {
        let sheets = MigrationTools::scan_json_folder(&json_folder).map_err(|e| e.to_string())?;
                let ordered = MigrationTools::order_sheets_by_dependency(&sheets);
                let total_sheets = ordered.len();
                let _ = tx_prog.send(MigrationProgress { total: total_sheets, completed: 0, current_sheet: None, message: "Starting migration...".into() });

                // Open or create database
                let mut conn = if create_new || !db_path.exists() {
                    super::connection::DbConnection::create_new(&db_path).map_err(|e| e.to_string())?
                } else {
                    rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?
                };
                let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");

                let mut report = super::migration::MigrationReport::default();
                for (idx, sheet_name) in ordered.iter().enumerate() {
                    let _ = tx_prog.send(MigrationProgress { total: total_sheets, completed: idx, current_sheet: Some(sheet_name.clone()), message: format!("Migrating '{}'...", sheet_name) });
                    if let Some(pair) = sheets.get(sheet_name) {
                        // Pre-scan data and estimate structure rows for better progress messages
                        let grid: Vec<Vec<String>> = std::fs::read_to_string(&pair.data_path)
                            .ok()
                            .and_then(|s| serde_json::from_str::<Vec<Vec<String>>>(&s).ok())
                            .unwrap_or_default();
                        let grid_rows: usize = grid.len();
                        let metadata: Option<crate::sheets::definitions::SheetMetadata> = std::fs::read_to_string(&pair.meta_path)
                            .ok()
                            .and_then(|s| serde_json::from_str::<crate::sheets::definitions::SheetMetadata>(&s).ok());
                        let structure_col_indices: Vec<usize> = metadata
                            .as_ref()
                            .map(|m| m.columns.iter().enumerate().filter_map(|(i, c)| if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { Some(i) } else { None }).collect())
                            .unwrap_or_default();
                        let mut struct_estimate: usize = 0;
                        if !structure_col_indices.is_empty() {
                            for row in &grid {
                                for &col_idx in &structure_col_indices {
                                    let cell = row.get(col_idx).cloned().unwrap_or_default();
                                    if cell.trim().is_empty() { continue; }
                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cell) {
                                        match val {
                                            serde_json::Value::Array(arr) => {
                                                if arr.iter().all(|v| v.is_array() || v.is_object()) { struct_estimate += arr.len(); }
                                            }
                                            serde_json::Value::Object(_) => { struct_estimate += 1; }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        let struct_tables_count = structure_col_indices.len();

                        let sheet_name_for_cb = sheet_name.clone();
                        let tx_prog_cb = tx_prog.clone();
                        let total_sheets_cb = total_sheets;
                        let mut row_notifier = move |rows_done: usize| {
                            let phase = if rows_done <= grid_rows { "main" } else { "structures" };
                            let structures_done = rows_done.saturating_sub(grid_rows);
                            let suffix = if phase == "structures" && struct_estimate > 0 {
                                if structures_done <= struct_estimate {
                                    format!(" ({} / {} structure rows)", structures_done, struct_estimate)
                                } else {
                                    format!(" ({} structure rows so far; est {})", structures_done, struct_estimate)
                                }
                            } else { String::new() };
                            let count_display = if phase == "main" { grid_rows.min(rows_done) } else { structures_done };
                            let _ = tx_prog_cb.send(MigrationProgress {
                                total: total_sheets_cb,
                                completed: idx,
                                current_sheet: Some(sheet_name_for_cb.clone()),
                                message: format!("{} ({}): {} rows...{}", sheet_name_for_cb, phase, count_display, suffix),
                            });
                        };
                        match MigrationTools::migrate_sheet_from_json(&mut conn, &pair.data_path, &pair.meta_path, sheet_name, Some(idx as i32), Some(&mut row_notifier)) {
                            Ok(_) => {
                                report.sheets_migrated += 1;
                                for dep in &pair.dependencies {
                                    if !report.linked_sheets_found.contains(dep) {
                                        report.linked_sheets_found.push(dep.clone());
                                    }
                                }
                                // Mark sheet-level completion
                                // Compute actual inserted structure rows and per-table breakdown
                                let mut per_table: Vec<(String, usize)> = Vec::new();
                                let actual_struct_rows: usize = metadata.as_ref().map(|m| {
                                    m.columns.iter().filter_map(|c| {
                                        if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) {
                                            let tname = format!("{}_{}", sheet_name, c.header);
                                            let count: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM \"{}\"", tname), [], |r| r.get(0)).unwrap_or(0);
                                            per_table.push((tname, count as usize));
                                            Some(count as usize)
                                        } else { None }
                                    }).sum()
                                }).unwrap_or(0);
                                let breakdown = if !per_table.is_empty() {
                                    let parts: Vec<String> = per_table.iter().map(|(n,c)| format!("{}={}", n, c)).collect();
                                    format!(" [{}]", parts.join(", "))
                                } else { String::new() };
                                let completion_msg = if struct_estimate > 0 {
                                    format!("Completed '{}' (main: {}, structures: {} rows across {} table(s)){}", sheet_name, grid_rows, actual_struct_rows, struct_tables_count, breakdown)
                                } else {
                                    format!("Completed '{}' ({} rows)", sheet_name, grid_rows)
                                };
                                let _ = tx_prog.send(MigrationProgress { total: total_sheets, completed: idx + 1, current_sheet: Some(sheet_name.clone()), message: completion_msg });
                            }
                            Err(e) => {
                                report.sheets_failed += 1;
                                report.failed_sheets.push((sheet_name.clone(), e.to_string()));
                            }
                        }
                    }
                }
                let _ = tx_prog.send(MigrationProgress { total: total_sheets, completed: total_sheets, current_sheet: None, message: "Finalizing...".into() });
                Ok((report, db_path))
            }();
            let _ = tx_done.send(run);
        });

    bg_state.progress_rx = Some(Arc::new(Mutex::new(rx_prog)));
    bg_state.completion_rx = Some(Arc::new(Mutex::new(rx_done)));
    }
}

pub fn poll_migration_background(
    mut bg_state: ResMut<MigrationBackgroundState>,
    mut progress_writer: EventWriter<MigrationProgress>,
    mut completed_writer: EventWriter<MigrationCompleted>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut registry: ResMut<crate::sheets::resources::SheetRegistry>,
    mut data_modified_writer: EventWriter<crate::sheets::events::SheetDataModifiedInRegistryEvent>,
    mut revalidate_writer: EventWriter<crate::sheets::events::RequestSheetRevalidation>,
    mut editor_state: Option<ResMut<crate::ui::elements::editor::state::EditorWindowState>>,
) {
    // Drain any progress updates
    if let Some(rx) = &bg_state.progress_rx {
        if let Ok(rx) = rx.lock() {
            for msg in rx.try_iter() {
                // For single-file imports, also mirror progress to feedback log instead of popup
                if bg_state.post_select.is_some() {
                    feedback_writer.write(SheetOperationFeedback { message: msg.message.clone(), is_error: false });
                    info!("{}", msg.message);
                }
                progress_writer.write(msg);
            }
        }
    }

    // Check for completion
    if let Some(rx) = &bg_state.completion_rx {
        let result = rx.lock().ok().and_then(|rx| rx.try_recv().ok());
        if let Some(res) = result {
            match res {
            Ok((report, db_path)) => {
                let success_msg = format!(
                    "Migration completed! {} sheets migrated, {} failed",
                    report.sheets_migrated, report.sheets_failed
                );
                info!("{}", success_msg);
                if !report.failed_sheets.is_empty() { for (name, err) in report.failed_sheets { warn!("Failed to migrate '{}': {}", name, err); } }
                feedback_writer.write(SheetOperationFeedback { message: success_msg.clone(), is_error: false });

                // Load tables into registry
                match rusqlite::Connection::open(&db_path) {
                    Ok(conn) => match crate::sheets::database::reader::DbReader::list_sheets(&conn) {
                        Ok(table_names) => {
                            let db_name = db_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Unknown".to_string());
                            for table_name in table_names.iter() {
                                match crate::sheets::database::reader::DbReader::read_metadata(&conn, table_name) {
                                    Ok(mut metadata) => {
                                        metadata.category = Some(db_name.clone());
                                        match crate::sheets::database::reader::DbReader::read_grid_data(&conn, table_name, &metadata) {
                                            Ok(grid) => {
                                                let sheet_data = crate::sheets::definitions::SheetGridData { grid, metadata: Some(metadata.clone()) };
                                                registry.add_or_replace_sheet(metadata.category.clone(), table_name.clone(), sheet_data);
                                                data_modified_writer.write(crate::sheets::events::SheetDataModifiedInRegistryEvent { category: Some(db_name.clone()), sheet_name: table_name.clone() });
                                                revalidate_writer.write(crate::sheets::events::RequestSheetRevalidation { category: Some(db_name.clone()), sheet_name: table_name.clone() });
                                            }
                                            Err(e) => error!("Post-migration: Failed to read grid for '{}': {}", table_name, e),
                                        }
                                    }
                                    Err(e) => error!("Post-migration: Failed to read metadata for '{}': {}", table_name, e),
                                }
                            }
                            // Auto-select preferred table if provided; otherwise first table
                            let mut selected_table: Option<String> = None;
                            if let Some((cat, name)) = bg_state.post_select.take() {
                                if cat == db_name && table_names.iter().any(|t| t == &name) {
                                    selected_table = Some(name);
                                }
                            }
                            if selected_table.is_none() {
                                selected_table = table_names.first().cloned();
                            }
                            if let (Some(sel_table), Some(state)) = (selected_table, editor_state.as_deref_mut()) {
                                state.selected_category = Some(db_name.clone());
                                state.selected_sheet_name = Some(sel_table.clone());
                                state.reset_interaction_modes_and_selections();
                                state.force_filter_recalculation = true;
                                info!("Auto-selected migrated DB table '{:?}/{}'", state.selected_category, sel_table);
                            }
                        }
                        Err(e) => error!("Post-migration: Failed to list tables: {}", e),
                    },
                    Err(e) => error!("Post-migration: Failed to open DB '{}': {}", db_path.display(), e),
                }

                completed_writer.write(MigrationCompleted { success: true, report: success_msg });
                bg_state.progress_rx = None;
                bg_state.completion_rx = None;
            }
            Err(err) => {
                let error_msg = format!("Migration failed: {}", err);
                error!("{}", error_msg);
                feedback_writer.write(SheetOperationFeedback { message: error_msg.clone(), is_error: true });
                completed_writer.write(MigrationCompleted { success: false, report: error_msg });
                bg_state.progress_rx = None;
                bg_state.completion_rx = None;
            }
            }
        } 
    }
}

pub fn handle_export_requests(
    mut events: EventReader<RequestExportSheetToJson>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    for event in events.read() {
        info!("Exporting table '{}' from {:?} to {:?}", 
              event.table_name, event.db_path, event.output_folder);
        
        match rusqlite::Connection::open(&event.db_path) {
            Ok(conn) => {
                match MigrationTools::export_sheet_to_json(
                    &conn,
                    &event.table_name,
                    &event.output_folder,
                ) {
                    Ok(_) => {
                        let msg = format!("Successfully exported '{}' to JSON", event.table_name);
                        info!("{}", msg);
                        feedback_writer.write(SheetOperationFeedback {
                            message: msg,
                            is_error: false,
                        });
                    }
                    Err(e) => {
                        let msg = format!("Failed to export '{}': {}", event.table_name, e);
                        error!("{}", msg);
                        feedback_writer.write(SheetOperationFeedback {
                            message: msg,
                            is_error: true,
                        });
                    }
                }
            }
            Err(e) => {
                let msg = format!("Failed to open database: {}", e);
                error!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        }
    }
}

pub fn handle_migration_completion(
    mut events: EventReader<MigrationCompleted>,
    mut migration_state: ResMut<crate::ui::elements::popups::MigrationPopupState>,
) {
    for _event in events.read() {
        migration_state.migration_in_progress = false;
        // Could add more UI feedback here
    }
}

/// Handle uploading a single JSON file and migrating it into the current database
pub fn handle_upload_json_to_current_db(
    mut events: EventReader<RequestUploadJsonToCurrentDb>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut registry: ResMut<crate::sheets::resources::SheetRegistry>,
    mut data_modified_writer: EventWriter<crate::sheets::events::SheetDataModifiedInRegistryEvent>,
    mut revalidate_writer: EventWriter<crate::sheets::events::RequestSheetRevalidation>,
    mut editor_state: Option<ResMut<crate::ui::elements::editor::state::EditorWindowState>>,
    mut bg_state: ResMut<MigrationBackgroundState>,
) {
    for event in events.read() {
        let db_name = &event.target_db_name;
        
        info!("Initiating JSON file upload for database '{}'", db_name);
        
        // Open file dialog for JSON file
        let picked_file: Option<PathBuf> = rfd::FileDialog::new()
            .add_filter("JSON files", &["json"])
            .set_title("Select JSON sheet file to import")
            .pick_file();
        
        let json_path = match picked_file {
            Some(path) => {
                // Check if user accidentally picked a .meta.json file
                if path.file_name()
                    .map_or(false, |name| name.to_string_lossy().ends_with(".meta.json"))
                {
                    feedback_writer.write(SheetOperationFeedback {
                        message: "Please select the main data file (.json), not the metadata file (.meta.json)".to_string(),
                        is_error: true,
                    });
                    continue;
                }
                path
            }
            None => {
                feedback_writer.write(SheetOperationFeedback {
                    message: "File selection cancelled".to_string(),
                    is_error: false,
                });
                continue;
            }
        };
        
        // Derive table name from file name
        let table_name = json_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("imported_sheet")
            .to_string();
        
        // Find corresponding .meta.json file
        let meta_path = json_path.with_file_name(format!("{}.meta.json", table_name));
        
        if !meta_path.exists() {
            feedback_writer.write(SheetOperationFeedback {
                message: format!("Metadata file not found: {}. Both .json and .meta.json files are required.", meta_path.display()),
                is_error: true,
            });
            continue;
        }
        
        // Open database connection
        let base_path = get_default_data_base_path();
        let db_path = base_path.join(format!("{}.db", db_name));
        
        if !db_path.exists() {
            feedback_writer.write(SheetOperationFeedback {
                message: format!("Database '{}' not found", db_name),
                is_error: true,
            });
            continue;
        }
        
        // Start background import with progress updates (including per-1k rows)
        feedback_writer.write(SheetOperationFeedback {
            message: format!("Import started for '{}' into database '{}'", table_name, db_name),
            is_error: false,
        });

        let (tx_prog, rx_prog) = channel::<MigrationProgress>();
        let (tx_done, rx_done) = channel::<Result<(super::migration::MigrationReport, PathBuf), String>>();

    let db_path_clone = db_path.clone();
    let json_path_clone = json_path.clone();
    let meta_path_clone = meta_path.clone();
    let table_name_clone = table_name.clone();

        thread::spawn(move || {
            let run = || -> Result<(super::migration::MigrationReport, PathBuf), String> {
                let _ = tx_prog.send(MigrationProgress { total: 1, completed: 0, current_sheet: Some(table_name_clone.clone()), message: format!("Migrating '{}'...", table_name_clone) });

                let mut conn = rusqlite::Connection::open(&db_path_clone).map_err(|e| e.to_string())?;
                let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");

                // Determine display order by counting existing tables
                let mut report = super::migration::MigrationReport::default();

                // Read row count for progress info (best-effort)
                let grid: Vec<Vec<String>> = std::fs::read_to_string(&json_path_clone)
                    .ok()
                    .and_then(|s| serde_json::from_str::<Vec<Vec<String>>>(&s).ok())
                    .unwrap_or_default();
                let grid_rows: usize = grid.len();
                let metadata: Option<crate::sheets::definitions::SheetMetadata> = std::fs::read_to_string(&meta_path_clone)
                    .ok()
                    .and_then(|s| serde_json::from_str::<crate::sheets::definitions::SheetMetadata>(&s).ok());
                let structure_col_indices: Vec<usize> = metadata
                    .as_ref()
                    .map(|m| m.columns.iter().enumerate().filter_map(|(i, c)| if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { Some(i) } else { None }).collect())
                    .unwrap_or_default();
                let mut struct_estimate: usize = 0;
                if !structure_col_indices.is_empty() {
                    for row in &grid {
                        for &col_idx in &structure_col_indices {
                            let cell = row.get(col_idx).cloned().unwrap_or_default();
                            if cell.trim().is_empty() { continue; }
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cell) {
                                match val {
                                    serde_json::Value::Array(arr) => {
                                        if arr.iter().all(|v| v.is_array() || v.is_object()) { struct_estimate += arr.len(); }
                                    }
                                    serde_json::Value::Object(_) => { struct_estimate += 1; }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                let table_name_cb = table_name_clone.clone();
                let tx_prog_cb = tx_prog.clone();
                let mut row_notifier = move |rows_done: usize| {
                    let phase = if rows_done <= grid_rows { "main" } else { "structures" };
                    let structures_done = rows_done.saturating_sub(grid_rows);
                    let suffix = if phase == "structures" && struct_estimate > 0 {
                        if structures_done <= struct_estimate {
                            format!(" ({} / {} structure rows)", structures_done, struct_estimate)
                        } else {
                            format!(" ({} structure rows so far; est {})", structures_done, struct_estimate)
                        }
                    } else { String::new() };
                    let count_display = if phase == "main" { grid_rows.min(rows_done) } else { structures_done };
                    let _ = tx_prog_cb.send(MigrationProgress {
                        total: 1,
                        completed: 0,
                        current_sheet: Some(table_name_cb.clone()),
                        message: format!("{} ({}): {} rows...{}", table_name_cb, phase, count_display, suffix),
                    });
                };

                match MigrationTools::migrate_sheet_from_json(&mut conn, &json_path_clone, &meta_path_clone, &table_name_clone, None, Some(&mut row_notifier)) {
                    Ok(_) => {
                        report.sheets_migrated += 1;
                        // Compute actual inserted structure rows and per-table breakdown
                        let mut per_table: Vec<(String, usize)> = Vec::new();
                        let actual_struct_rows: usize = metadata.as_ref().map(|m| {
                            m.columns.iter().filter_map(|c| {
                                if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) {
                                    let tname = format!("{}_{}", table_name_clone, c.header);
                                    let count: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM \"{}\"", tname), [], |r| r.get(0)).unwrap_or(0);
                                    per_table.push((tname, count as usize));
                                    Some(count as usize)
                                } else { None }
                            }).sum()
                        }).unwrap_or(0);
                        let breakdown = if !per_table.is_empty() {
                            let parts: Vec<String> = per_table.iter().map(|(n,c)| format!("{}={}", n, c)).collect();
                            format!(" [{}]", parts.join(", "))
                        } else { String::new() };
                        let completion_msg = if struct_estimate > 0 {
                            format!("Completed '{}' (main: {}, structures: {} rows){}", table_name_clone, grid_rows, actual_struct_rows, breakdown)
                        } else {
                            format!("Completed '{}' ({} rows)", table_name_clone, grid_rows)
                        };
                        let _ = tx_prog.send(MigrationProgress { total: 1, completed: 1, current_sheet: Some(table_name_clone.clone()), message: completion_msg });
                    }
                    Err(e) => {
                        report.sheets_failed += 1;
                        report.failed_sheets.push((table_name_clone.clone(), e.to_string()));
                        let _ = tx_prog.send(MigrationProgress { total: 1, completed: 1, current_sheet: Some(table_name_clone.clone()), message: format!("Failed '{}'", table_name_clone) });
                    }
                }

                Ok((report, db_path_clone))
            }();
            let _ = tx_done.send(run);
        });

        // Register background channels and preferred selection for the poller
        bg_state.progress_rx = Some(Arc::new(Mutex::new(rx_prog)));
        bg_state.completion_rx = Some(Arc::new(Mutex::new(rx_done)));
        bg_state.post_select = Some((db_name.clone(), table_name.clone()));
    }
}
