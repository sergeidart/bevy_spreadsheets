// src/sheets/systems/logic/delete_columns.rs
use crate::sheets::{
    definitions::{ColumnValidator, SheetMetadata},
    events::{
        RequestDeleteColumns, RequestDeleteSheetFile, SheetDataModifiedInRegistryEvent,
        SheetOperationFeedback,
    },
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use crate::sheets::database::open_or_create_db_for_category;
use std::collections::HashMap;
use std::path::PathBuf;

pub fn handle_delete_columns_request(
    mut events: EventReader<RequestDeleteColumns>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut file_delete_writer: EventWriter<RequestDeleteSheetFile>,
    editor_state: Option<Res<EditorWindowState>>,
) {
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();
    let mut structure_sheets_to_delete: Vec<(Option<String>, String)> = Vec::new();
    // Collect DB-backed column deletion requests: (category, table_name, column_name)
    let mut db_column_deletions: Vec<(String, String, String)> = Vec::new();

    for event in events.read() {
        // Process delete event; scheduled DB column deletions will be recorded in outer vector
        let (category, sheet_name) = if let Some(state) = editor_state.as_ref() {
            if let Some(vctx) = state.virtual_structure_stack.last() {
                (&event.category, &vctx.virtual_sheet_name)
            } else {
                (&event.category, &event.sheet_name)
            }
        } else {
            (&event.category, &event.sheet_name)
        };
        let indices_to_delete = &event.column_indices;

        if indices_to_delete.is_empty() {
            trace!(
                "Skipping delete columns request for '{:?}/{}': No indices provided.",
                category,
                sheet_name
            );
            continue;
        }

        let mut operation_successful = false;
        let mut deleted_count = 0;
        let mut error_message: Option<String> = None;
        let mut metadata_cache: Option<SheetMetadata> = None;

        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &mut sheet_data.metadata {
                // Sort indices descending to avoid shifting issues during removal
                let mut sorted_indices: Vec<usize> = indices_to_delete.iter().cloned().collect();
                sorted_indices.sort_unstable_by(|a, b| b.cmp(a)); // Sort descending

                let initial_col_count = metadata.columns.len();

                for &col_idx_to_remove in &sorted_indices {
                    if col_idx_to_remove < metadata.columns.len() {
                        // Mark column as deleted for reuse and disable AI inclusion
                        if let Some(col_def) = metadata.columns.get_mut(col_idx_to_remove) {
                            col_def.deleted = true;
                            col_def.ai_include_in_send = Some(false);
                            // Schedule metadata update for DB-backed
                            if let Some(cat_str) = category.clone() {
                                db_column_deletions.push((cat_str, sheet_name.clone(), col_def.header.clone()));
                            }
                            // Check for structure validator cascade
                            if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                                let structure_sheet_name = format!("{}_{}", sheet_name, col_def.header);
                                structure_sheets_to_delete.push((category.clone(), structure_sheet_name));
                            }
                        }
                        deleted_count += 1;
                    } else {
                        let err_msg = format!(
                            "Column index {} out of bounds ({} columns). Deletion partially failed.",
                            col_idx_to_remove, initial_col_count
                        );
                        error_message = Some(err_msg.clone());
                        warn!(
                            "Skipping delete for column index {} in '{:?}/{}': Out of bounds. {}",
                            col_idx_to_remove, category, sheet_name, err_msg
                        );
                    }
                }

                if deleted_count > 0 {
                    metadata.ensure_column_consistency(); // Recalculate consistency if needed
                    operation_successful = true;
                    metadata_cache = Some(metadata.clone());
                    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                        category: category.clone(),
                        sheet_name: sheet_name.clone(),
                    });
                }
            } else {
                error_message = Some(format!(
                    "Metadata missing for sheet '{:?}/{}'. Cannot delete columns.",
                    category, sheet_name
                ));
            }
        } else {
            error_message = Some(format!(
                "Sheet '{:?}/{}' not found. Cannot delete columns.",
                category, sheet_name
            ));
        }

        if operation_successful {
            let base_msg = format!(
                "Deleted {} column(s) from sheet '{:?}/{}'.",
                deleted_count, category, sheet_name
            );
            let final_msg = if let Some(ref err) = error_message {
                format!("{} {}", base_msg, err)
            } else {
                base_msg
            };
            info!("{}", final_msg);
            feedback_writer.write(SheetOperationFeedback {
                message: final_msg,
                is_error: error_message.is_some(),
            });

            if let Some(meta) = metadata_cache {
                sheets_to_save.insert((category.clone(), sheet_name.clone()), meta);
            } else if deleted_count > 0 {
                warn!(
                    "Columns deleted from '{:?}/{}' but cannot save: Metadata missing or no columns actually deleted from metadata.",
                    category, sheet_name
                );
            }
        } else if let Some(err) = error_message {
            error!(
                "Failed to delete columns from '{:?}/{}': {}",
                category, sheet_name, err
            );
            feedback_writer.write(SheetOperationFeedback {
                message: format!(
                    "Column delete failed for '{:?}/{}': {}",
                    category, sheet_name, err
                ),
                is_error: true,
            });
        }
    }

    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!("Columns deleted in '{:?}/{}', triggering save.", cat, name);
            if metadata.category.is_none() {
                save_single_sheet(registry_immut, &metadata);
            }
        }
    }

    // Cascade delete structure sheets
    if !structure_sheets_to_delete.is_empty() {
        for (struct_category, struct_sheet_name) in structure_sheets_to_delete {
            // Remove from registry
            if let Ok(_removed) = registry.delete_sheet(&struct_category, &struct_sheet_name) {
                info!(
                    "Removed structure sheet '{:?}/{}' from registry due to cascade delete.",
                    struct_category, struct_sheet_name
                );

                // Delete physical files (.json and .meta.json)
                let category_path = if let Some(ref cat) = struct_category {
                    format!("{}/", cat)
                } else {
                    String::new()
                };

                let json_path =
                    PathBuf::from(format!("{}{}.json", category_path, struct_sheet_name));
                let meta_path =
                    PathBuf::from(format!("{}{}.meta.json", category_path, struct_sheet_name));

                file_delete_writer.write(RequestDeleteSheetFile {
                    relative_path: json_path,
                });
                file_delete_writer.write(RequestDeleteSheetFile {
                    relative_path: meta_path,
                });

                // If DB-backed, also remove the structure table and its metadata rows
                if let Some(cat) = &struct_category {
                    let base = crate::sheets::systems::io::get_default_data_base_path();
                    let db_path = base.join(format!("{}.db", cat));
                    if db_path.exists() {
                        if let Ok(conn) = crate::sheets::database::connection::DbConnection::open_existing(&db_path) {
                            // Drop the structure data table if exists
                            let drop_sql = format!("DROP TABLE IF EXISTS \"{}\"", struct_sheet_name);
                            if let Err(e) = conn.execute(&drop_sql, []) {
                                warn!("Failed to drop structure table '{}' in DB '{}': {}", struct_sheet_name, db_path.display(), e);
                            } else {
                                info!("Dropped structure table '{}' from DB '{}'.", struct_sheet_name, db_path.display());
                            }
                            // Remove metadata entry for the structure table from _Metadata
                            if let Err(e) = conn.execute("DELETE FROM _Metadata WHERE table_name = ?", rusqlite::params![struct_sheet_name]) {
                                warn!("Failed to remove _Metadata entry for '{}' in DB '{}': {}", struct_sheet_name, db_path.display(), e);
                            } else {
                                info!("Removed _Metadata entry for '{}' from DB '{}'.", struct_sheet_name, db_path.display());
                            }
                            // Drop per-table metadata table if exists
                            let meta_table = format!("{}_Metadata", struct_sheet_name);
                            let drop_meta_sql = format!("DROP TABLE IF EXISTS \"{}\"", meta_table);
                            if let Err(e) = conn.execute(&drop_meta_sql, []) {
                                warn!("Failed to drop metadata table '{}' in DB '{}': {}", meta_table, db_path.display(), e);
                            } else {
                                info!("Dropped metadata table '{}' from DB '{}'.", meta_table, db_path.display());
                            }
                        }
                    }
                }

                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "Cascade deleted structure sheet '{:?}/{}'.",
                        struct_category, struct_sheet_name
                    ),
                    is_error: false,
                });
            } else {
                warn!(
                    "Attempted to cascade delete structure sheet '{:?}/{}' but it was not found in registry.",
                    struct_category, struct_sheet_name
                );
            }
        }
    }

    // Cascade drop columns for DB-backed sheets
    // Process DB-backed column deletions: remove metadata entries only
    if !db_column_deletions.is_empty() {
        info!("Marking {} DB-backed column(s) deleted: {:?}", db_column_deletions.len(), db_column_deletions);
        for (cat, table_name, column_name) in db_column_deletions {
            match open_or_create_db_for_category(&cat) {
                Ok(conn) => {
                    let meta_table = format!("{}_Metadata", table_name);
                    // Mark deleted flag and disable AI include in metadata
                    let update_sql = format!(
                        "UPDATE \"{}\" SET deleted = 1, ai_include_in_send = 0 WHERE column_name = ?",
                        meta_table
                    );
                    match conn.execute(&update_sql, rusqlite::params![column_name]) {
                        Ok(count) => {
                            info!("Marked {} row(s) deleted in '{}', AI include disabled", count, meta_table);
                            // Also wipe column contents to free space immediately, but first verify column exists
                            let mut col_exists = false;
                            if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name)) {
                                if let Ok(mut rows) = stmt.query([]) {
                                    while let Ok(Some(row)) = rows.next() {
                                        let name: String = row.get(1).unwrap_or_default();
                                        if name == column_name {
                                            col_exists = true;
                                            break;
                                        }
                                    }
                                }
                            }
                            if col_exists {
                                // Wipe data first
                                let wipe_sql = format!("UPDATE \"{}\" SET \"{}\" = NULL", table_name, column_name);
                                match conn.execute(&wipe_sql, []) {
                                    Ok(wiped) => {
                                        info!("Wiped {} cell(s) in '{}' column '{}'", wiped, table_name, column_name);
                                        
                                        // Try to drop the column (SQLite 3.35.0+)
                                        let drop_sql = format!("ALTER TABLE \"{}\" DROP COLUMN \"{}\"", table_name, column_name);
                                        match conn.execute(&drop_sql, []) {
                                            Ok(_) => {
                                                info!("Dropped column '{}' from table '{}'", column_name, table_name);
                                                
                                                // Run VACUUM to reclaim space
                                                match conn.execute("VACUUM", []) {
                                                    Ok(_) => info!("VACUUM completed after dropping column '{}'", column_name),
                                                    Err(e) => warn!("VACUUM failed after dropping column '{}': {}", column_name, e),
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Failed to drop column '{}' from '{}': {} (data wiped, column still exists)", column_name, table_name, e);
                                            }
                                        }
                                    }
                                    Err(e) => warn!("Failed to wipe data for column '{}' on '{}': {}", column_name, table_name, e),
                                }
                            } else {
                                warn!("Cannot wipe data for column '{}' on '{}': column not found in table schema", column_name, table_name);
                            }
                        }
                        Err(e) => warn!(
                            "Failed to mark column '{}' deleted in '{}': {}",
                            column_name, meta_table, e
                        ),
                    }
                }
                Err(e) => warn!("Could not open DB for category '{}' when marking column '{}' deleted: {}", cat, column_name, e),
            }
        }
    }
}
