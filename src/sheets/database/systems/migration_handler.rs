// src/sheets/database/systems/migration_handler.rs

use super::super::migration::MigrationTools;
use super::MigrationBackgroundState;
use crate::sheets::events::{MigrationProgress, RequestMigrateJsonToDb, SheetOperationFeedback};
use bevy::prelude::*;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;

/// Handle requests to migrate JSON sheets to SQLite database
pub fn handle_migration_requests(
    mut events: EventReader<RequestMigrateJsonToDb>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut bg_state: ResMut<MigrationBackgroundState>,
) {
    for event in events.read() {
        info!(
            "Starting migration from {:?} to {:?}",
            event.json_folder_path, event.target_db_path
        );
        // Spawn background thread and set up channels
        let (tx_prog, rx_prog) = channel::<MigrationProgress>();
        let (tx_done, rx_done) =
            channel::<Result<(super::super::migration::MigrationReport, PathBuf), String>>();

        let json_folder = event.json_folder_path.clone();
        let db_path = event.target_db_path.clone();
        let create_new = event.create_new_db;

        // Send initial info message
        feedback_writer.write(SheetOperationFeedback {
            message: "Migration started...".into(),
            is_error: false,
        });

        thread::spawn(move || {
            let run = || -> Result<(super::super::migration::MigrationReport, PathBuf), String> {
                let sheets =
                    MigrationTools::scan_json_folder(&json_folder).map_err(|e| e.to_string())?;
                let ordered = MigrationTools::order_sheets_by_dependency(&sheets);
                let total_sheets = ordered.len();
                let _ = tx_prog.send(MigrationProgress {
                    total: total_sheets,
                    completed: 0,
                    current_sheet: None,
                    message: "Starting migration...".into(),
                });

                // Open or create database
                let mut conn = if create_new || !db_path.exists() {
                    super::super::connection::DbConnection::create_new(&db_path)
                        .map_err(|e| e.to_string())?
                } else {
                    rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?
                };
                let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");

                let mut report = super::super::migration::MigrationReport::default();
                for (idx, sheet_name) in ordered.iter().enumerate() {
                    let _ = tx_prog.send(MigrationProgress {
                        total: total_sheets,
                        completed: idx,
                        current_sheet: Some(sheet_name.clone()),
                        message: format!("Migrating '{}'...", sheet_name),
                    });
                    if let Some(pair) = sheets.get(sheet_name) {
                        // Pre-scan data and estimate structure rows for better progress messages
                        let grid: Vec<Vec<String>> = std::fs::read_to_string(&pair.data_path)
                            .ok()
                            .and_then(|s| serde_json::from_str::<Vec<Vec<String>>>(&s).ok())
                            .unwrap_or_default();
                        let grid_rows: usize = grid.len();
                        let metadata: Option<crate::sheets::definitions::SheetMetadata> =
                            std::fs::read_to_string(&pair.meta_path).ok().and_then(|s| {
                                serde_json::from_str::<crate::sheets::definitions::SheetMetadata>(
                                    &s,
                                )
                                .ok()
                            });
                        let structure_col_indices: Vec<usize> = metadata
                            .as_ref()
                            .map(|m| m.columns.iter().enumerate().filter_map(|(i, c)| if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { Some(i) } else { None }).collect())
                            .unwrap_or_default();
                        let mut struct_estimate: usize = 0;
                        if !structure_col_indices.is_empty() {
                            for row in &grid {
                                for &col_idx in &structure_col_indices {
                                    let cell = row.get(col_idx).cloned().unwrap_or_default();
                                    if cell.trim().is_empty() {
                                        continue;
                                    }
                                    if let Ok(val) =
                                        serde_json::from_str::<serde_json::Value>(&cell)
                                    {
                                        match val {
                                            serde_json::Value::Array(arr) => {
                                                if arr.iter().all(|v| v.is_array() || v.is_object())
                                                {
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
                        let struct_tables_count = structure_col_indices.len();

                        let sheet_name_for_cb = sheet_name.clone();
                        let tx_prog_cb = tx_prog.clone();
                        let total_sheets_cb = total_sheets;
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
                                total: total_sheets_cb,
                                completed: idx,
                                current_sheet: Some(sheet_name_for_cb.clone()),
                                message: format!(
                                    "{} ({}): {} rows...{}",
                                    sheet_name_for_cb, phase, count_display, suffix
                                ),
                            });
                        };
                        match MigrationTools::migrate_sheet_from_json(
                            &mut conn,
                            &pair.data_path,
                            &pair.meta_path,
                            sheet_name,
                            Some(idx as i32),
                            Some(&mut row_notifier),
                        ) {
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
                                    let parts: Vec<String> = per_table
                                        .iter()
                                        .map(|(n, c)| format!("{}={}", n, c))
                                        .collect();
                                    format!(" [{}]", parts.join(", "))
                                } else {
                                    String::new()
                                };
                                let completion_msg = if struct_estimate > 0 {
                                    format!("Completed '{}' (main: {}, structures: {} rows across {} table(s)){}", sheet_name, grid_rows, actual_struct_rows, struct_tables_count, breakdown)
                                } else {
                                    format!("Completed '{}' ({} rows)", sheet_name, grid_rows)
                                };
                                let _ = tx_prog.send(MigrationProgress {
                                    total: total_sheets,
                                    completed: idx + 1,
                                    current_sheet: Some(sheet_name.clone()),
                                    message: completion_msg,
                                });
                            }
                            Err(e) => {
                                report.sheets_failed += 1;
                                report
                                    .failed_sheets
                                    .push((sheet_name.clone(), e.to_string()));
                            }
                        }
                    }
                }
                
                // Apply occasional migration fixes
                let _ = tx_prog.send(MigrationProgress {
                    total: total_sheets,
                    completed: total_sheets,
                    current_sheet: None,
                    message: "Applying migration fixes...".into(),
                });
                
                let mut fix_manager = super::super::migration::OccasionalFixManager::new();
                fix_manager.register_fix(Box::new(
                    super::super::migration::fix_row_index_duplicates::FixRowIndexDuplicates
                ));
                
                match fix_manager.apply_all_fixes(&mut conn) {
                    Ok(applied) => {
                        if !applied.is_empty() {
                            info!("Applied migration fixes: {:?}", applied);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to apply some migration fixes: {:?}", e);
                    }
                }
                
                // Validate row_index integrity
                let _ = tx_prog.send(MigrationProgress {
                    total: total_sheets,
                    completed: total_sheets,
                    current_sheet: None,
                    message: "Validating row_index integrity...".into(),
                });
                
                match super::super::validation::validate_all_tables(&conn) {
                    Ok(results) => {
                        super::super::validation::log_validation_report(&results);
                    }
                    Err(e) => {
                        warn!("Failed to validate row_index: {:?}", e);
                    }
                }
                
                let _ = tx_prog.send(MigrationProgress {
                    total: total_sheets,
                    completed: total_sheets,
                    current_sheet: None,
                    message: "Finalizing...".into(),
                });
                Ok((report, db_path))
            }();
            let _ = tx_done.send(run);
        });

        bg_state.progress_rx = Some(Arc::new(Mutex::new(rx_prog)));
        bg_state.completion_rx = Some(Arc::new(Mutex::new(rx_done)));
    }
}
