// src/sheets/database/systems/upload_handler.rs

use super::super::migration::MigrationTools;
use super::MigrationBackgroundState;
use crate::sheets::events::{MigrationProgress, RequestUploadJsonToCurrentDb, SheetOperationFeedback};
use crate::sheets::systems::io::get_default_data_base_path;
use bevy::prelude::*;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;

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
                if path
                    .file_name()
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
                message: format!(
                    "Metadata file not found: {}. Both .json and .meta.json files are required.",
                    meta_path.display()
                ),
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
            message: format!(
                "Import started for '{}' into database '{}'",
                table_name, db_name
            ),
            is_error: false,
        });

        let (tx_prog, rx_prog) = channel::<MigrationProgress>();
        let (tx_done, rx_done) =
            channel::<Result<(super::super::migration::MigrationReport, PathBuf), String>>();

        let db_path_clone = db_path.clone();
        let json_path_clone = json_path.clone();
        let meta_path_clone = meta_path.clone();
        let table_name_clone = table_name.clone();

        thread::spawn(move || {
            let run = || -> Result<(super::super::migration::MigrationReport, PathBuf), String> {
                let _ = tx_prog.send(MigrationProgress {
                    total: 1,
                    completed: 0,
                    current_sheet: Some(table_name_clone.clone()),
                    message: format!("Migrating '{}'...", table_name_clone),
                });

                let mut conn =
                    rusqlite::Connection::open(&db_path_clone).map_err(|e| e.to_string())?;
                let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");

                // Determine display order by counting existing tables
                let mut report = super::super::migration::MigrationReport::default();

                // Read row count for progress info (best-effort)
                let grid: Vec<Vec<String>> = std::fs::read_to_string(&json_path_clone)
                    .ok()
                    .and_then(|s| serde_json::from_str::<Vec<Vec<String>>>(&s).ok())
                    .unwrap_or_default();
                let grid_rows: usize = grid.len();
                let metadata: Option<crate::sheets::definitions::SheetMetadata> =
                    std::fs::read_to_string(&meta_path_clone)
                        .ok()
                        .and_then(|s| {
                            serde_json::from_str::<crate::sheets::definitions::SheetMetadata>(&s)
                                .ok()
                        });
                let structure_col_indices: Vec<usize> = metadata
                    .as_ref()
                    .map(|m| {
                        m.columns
                            .iter()
                            .enumerate()
                            .filter_map(|(i, c)| {
                                if matches!(
                                    c.validator,
                                    Some(crate::sheets::definitions::ColumnValidator::Structure)
                                ) {
                                    Some(i)
                                } else {
                                    None
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut struct_estimate: usize = 0;
                if !structure_col_indices.is_empty() {
                    for row in &grid {
                        for &col_idx in &structure_col_indices {
                            let cell = row.get(col_idx).cloned().unwrap_or_default();
                            if cell.trim().is_empty() {
                                continue;
                            }
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cell) {
                                match val {
                                    serde_json::Value::Array(arr) => {
                                        if arr.iter().all(|v| v.is_array() || v.is_object()) {
                                            struct_estimate += arr.len();
                                        }
                                    }
                                    serde_json::Value::Object(_) => {
                                        struct_estimate += 1;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                let table_name_cb = table_name_clone.clone();
                let tx_prog_cb = tx_prog.clone();
                let mut row_notifier = move |rows_done: usize| {
                    let phase = if rows_done <= grid_rows {
                        "main"
                    } else {
                        "structures"
                    };
                    let structures_done = rows_done.saturating_sub(grid_rows);
                    let suffix = if phase == "structures" && struct_estimate > 0 {
                        if structures_done <= struct_estimate {
                            format!(
                                " ({} / {} structure rows)",
                                structures_done, struct_estimate
                            )
                        } else {
                            format!(
                                " ({} structure rows so far; est {})",
                                structures_done, struct_estimate
                            )
                        }
                    } else {
                        String::new()
                    };
                    let count_display = if phase == "main" {
                        grid_rows.min(rows_done)
                    } else {
                        structures_done
                    };
                    let _ = tx_prog_cb.send(MigrationProgress {
                        total: 1,
                        completed: 0,
                        current_sheet: Some(table_name_cb.clone()),
                        message: format!(
                            "{} ({}): {} rows...{}",
                            table_name_cb, phase, count_display, suffix
                        ),
                    });
                };

                match MigrationTools::migrate_sheet_from_json(
                    &mut conn,
                    &json_path_clone,
                    &meta_path_clone,
                    &table_name_clone,
                    None,
                    Some(&mut row_notifier),
                ) {
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
                            let parts: Vec<String> = per_table
                                .iter()
                                .map(|(n, c)| format!("{}={}", n, c))
                                .collect();
                            format!(" [{}]", parts.join(", "))
                        } else {
                            String::new()
                        };
                        let completion_msg = if struct_estimate > 0 {
                            format!(
                                "Completed '{}' (main: {}, structures: {} rows){}",
                                table_name_clone, grid_rows, actual_struct_rows, breakdown
                            )
                        } else {
                            format!("Completed '{}' ({} rows)", table_name_clone, grid_rows)
                        };
                        let _ = tx_prog.send(MigrationProgress {
                            total: 1,
                            completed: 1,
                            current_sheet: Some(table_name_clone.clone()),
                            message: completion_msg,
                        });
                    }
                    Err(e) => {
                        report.sheets_failed += 1;
                        report
                            .failed_sheets
                            .push((table_name_clone.clone(), e.to_string()));
                        let _ = tx_prog.send(MigrationProgress {
                            total: 1,
                            completed: 1,
                            current_sheet: Some(table_name_clone.clone()),
                            message: format!("Failed '{}'", table_name_clone),
                        });
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
