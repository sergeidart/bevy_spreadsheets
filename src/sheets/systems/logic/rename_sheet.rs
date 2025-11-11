// src/sheets/systems/logic/rename_sheet.rs
use crate::sheets::{
    events::{
        RequestRenameCacheEntry, RequestRenameSheet, RequestRenameSheetFile, SheetDataModifiedInRegistryEvent,
        SheetOperationFeedback,
    },
    resources::SheetRegistry,
};
use bevy::prelude::*;
use rusqlite::OptionalExtension; // for .optional() on query_row results
use std::path::PathBuf; // Added for relative path

/// Handles requests to rename a sheet *within its category*.
pub fn handle_rename_request(
    mut events: EventReader<RequestRenameSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut file_rename_writer: EventWriter<RequestRenameSheetFile>,
    // Emit cache rename requests for children (forwarded later to RequestRenameSheet)
    mut cache_rename_writer: EventWriter<RequestRenameCacheEntry>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    daemon_client: Res<crate::sheets::database::daemon_resource::SharedDaemonClient>,
) {
    for event in events.read() {
        let category = &event.category; // <<< Get category
        let old_name = &event.old_name;
        let new_name = &event.new_name;
        info!(
            "Handling rename request: '{:?}/{}' -> '{:?}/{}'",
            category, old_name, category, new_name
        );

        // --- Get old filenames BEFORE attempting rename ---
        // This requires an immutable borrow first
        let (old_grid_filename_opt, old_meta_filename_opt, old_category_opt) = {
            let registry_immut = registry.as_ref(); // Immutable borrow
            registry_immut
                .get_sheet(category, old_name)
                .and_then(|d| d.metadata.as_ref())
                .map(|m| {
                    let grid_fn = m.data_filename.clone();
                    let meta_fn = format!("{}.meta.json", old_name); // Meta uses OLD name
                    (Some(grid_fn), Some(meta_fn), m.category.clone()) // Store old category too
                })
                .unwrap_or((None, None, None)) // Default if sheet/metadata not found
        };

        // Ensure the determined category matches the event category
        if old_category_opt != *category {
            error!(
                 "Rename failed: Mismatch between event category ({:?}) and sheet's metadata category ({:?}) for '{}'.",
                 category, old_category_opt, old_name
             );
            feedback_writer.write(SheetOperationFeedback {
                message: format!("Internal error during rename for '{}'.", old_name),
                is_error: true,
            });
            continue;
        }

        // --- Perform Rename in Registry (Mutable Borrow) ---
        // Renaming happens within the specified category
        let rename_result = registry.rename_sheet(category, old_name, new_name.clone());

        match rename_result {
            Ok(mut moved_data) => {
                // rename_sheet now returns the data with *updated* metadata
                let success_msg = format!(
                    "Successfully renamed sheet in registry: '{:?}/{}' -> '{:?}/{}'.",
                    category, old_name, category, new_name
                );
                info!("{}", success_msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: success_msg,
                    is_error: false,
                });

                // NOTE: Do NOT save JSON files here prior to file rename.
                // Saving first would create the new files and prevent fs::rename,
                // leading to duplicates (old + new). We'll only emit file rename
                // requests for JSON mode and skip pre-saving entirely.
                if let Some(meta_to_save) = &mut moved_data.metadata {
                    // Ensure category in moved data is correct before saving
                    if meta_to_save.category != *category {
                        warn!(
                            "Correcting category mismatch in renamed data before save for '{}'.",
                            new_name
                        );
                        meta_to_save.category = category.clone();
                    }
                    // In DB mode, also rename physical DB tables (main + descendants)
                    if let Some(db_name) = &meta_to_save.category {
                        info!(
                            "DB mode: Performing DB cascade rename for '{:?}/{}'",
                            category, new_name
                        );
                        let base_path = crate::sheets::systems::io::get_default_data_base_path();
                        let db_path = base_path.join(format!("{}.db", db_name));
                        if db_path.exists() {
                            match rusqlite::Connection::open(&db_path) {
                                Ok(conn) => {
                                    if let Err(e) = crate::sheets::database::writer::DbWriter::rename_table_and_descendants(
                                        &conn,
                                        old_name,
                                        new_name,
                                        daemon_client.client(),
                                    ) {
                                        error!("DB cascade rename failed for '{:?}/{}': {}", category, new_name, e);
                                        feedback_writer.write(SheetOperationFeedback {
                                            message: format!(
                                                "Database rename failed for '{:?}/{}': {}",
                                                category, new_name, e
                                            ),
                                            is_error: true,
                                        });
                                    } else {
                                        info!(
                                            "DB cascade rename completed for '{:?}/{}'",
                                            category, new_name
                                        );

                                        // If this is a structure table rename, also update the parent column header to match new suffix
                                        // Detect via _Metadata
                                        let meta_row: Option<(String, Option<String>, Option<String>)> = conn
                                            .query_row(
                                                "SELECT table_type, parent_table, parent_column FROM _Metadata WHERE table_name = ?",
                                                [new_name],
                                                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?)),
                                            )
                                            .optional()
                                            .ok()
                                            .flatten();

                                        if let Some((table_type, parent_table_opt, parent_column_opt)) = meta_row {
                                            if table_type.eq_ignore_ascii_case("structure") {
                                                if let (Some(parent_table), Some(parent_col_old)) = (parent_table_opt, parent_column_opt) {
                                                    // Derive new header from new_name: expected pattern '<parent_table>_<new_header>'
                                                    let prefix = format!("{}_", parent_table);
                                                    let new_header: String = if let Some(stripped) = new_name.strip_prefix(&prefix) {
                                                        stripped.to_string()
                                                    } else {
                                                        // Fallback to last segment after underscore
                                                        new_name
                                                            .rsplit_once('_')
                                                            .map(|(_, s)| s.to_string())
                                                            .unwrap_or_else(|| new_name.to_string())
                                                    };

                                                    // Update parent's metadata column name in DB by old name
                                                    match crate::sheets::database::writer::DbWriter::update_metadata_column_name_by_name(&conn, &parent_table, &parent_col_old, &new_header, daemon_client.client()) {
                                                        Ok(_) => {
                                                            // Update child _Metadata parent_column to reflect new header using daemon
                                                            let statements = vec![
                                                                crate::sheets::database::daemon_client::Statement {
                                                                    sql: "UPDATE _Metadata SET parent_column = ? WHERE table_name = ?".to_string(),
                                                                    params: vec![serde_json::json!(new_header), serde_json::json!(new_name)],
                                                                },
                                                            ];
                                                            
                                                            match daemon_client.client().exec_batch(statements, db_path.file_name().and_then(|n| n.to_str())) {
                                                                Ok(response) => {
                                                                    if response.error.is_some() {
                                                                        warn!("Daemon error updating _Metadata parent_column for '{}': {:?}", new_name, response.error);
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    warn!("Failed to update _Metadata parent_column via daemon for '{}': {:?}", new_name, e);
                                                                }
                                                            }

                                                            // Update in-memory parent column header/display and notify cache rebuild
                                                            if let Some(parent_data) = registry.get_sheet_mut(category, &parent_table) {
                                                                if let Some(parent_meta) = &mut parent_data.metadata {
                                                                    if let Some(col_def) = parent_meta.columns.iter_mut().find(|c| c.header.eq_ignore_ascii_case(&parent_col_old)) {
                                                                        col_def.header = new_header.clone();
                                                                        col_def.display_header = Some(new_header.clone());
                                                                    }
                                                                }
                                                            }
                                                            data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                                                                category: category.clone(),
                                                                sheet_name: parent_table.clone(),
                                                            });
                                                            info!(
                                                                "Synchronized parent column header: '{}.{}' -> '{}.{}'",
                                                                parent_table, parent_col_old, parent_table, new_header
                                                            );
                                                        }
                                                        Err(e) => {
                                                            warn!(
                                                                "Failed to update parent column header in DB for '{}.{}' -> '{}': {}",
                                                                parent_table, parent_col_old, new_header, e
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to open DB for cascade rename: {}", e);
                                }
                            }
                        } else {
                            warn!("Database file not found for cascade rename: {}", db_path.display());
                        }
                    } else {
                        info!(
                            "JSON mode: Skipping DB cascade rename for '{:?}/{}'",
                            category, new_name
                        );
                    }

                    // --- Request File Renames using updated metadata ---
                    let new_grid_filename = &meta_to_save.data_filename; // Already updated by rename_sheet
                    let new_meta_filename = format!("{}.meta.json", new_name); // Meta uses NEW name

                    // Construct relative paths based on the category (which hasn't changed)
                    let mut old_grid_rel_path = PathBuf::new();
                    let mut new_grid_rel_path = PathBuf::new();
                    let mut old_meta_rel_path = PathBuf::new();
                    let mut new_meta_rel_path = PathBuf::new();

                    if let Some(cat_name) = category {
                        old_grid_rel_path.push(cat_name);
                        new_grid_rel_path.push(cat_name);
                        old_meta_rel_path.push(cat_name);
                        new_meta_rel_path.push(cat_name);
                    }

                    // Add filenames
                    if let Some(old_grid_fn) = old_grid_filename_opt {
                        old_grid_rel_path.push(old_grid_fn);
                    }
                    if !new_grid_filename.is_empty() {
                        new_grid_rel_path.push(new_grid_filename);
                    }
                    if let Some(old_meta_fn) = old_meta_filename_opt {
                        old_meta_rel_path.push(old_meta_fn);
                    }
                    new_meta_rel_path.push(new_meta_filename);

                    // JSON mode: request fs renames (grid)
                    if meta_to_save.category.is_none() {
                        // Request grid file rename only if old name existed and names differ
                        if !old_grid_rel_path.as_os_str().is_empty()
                            && !new_grid_rel_path.as_os_str().is_empty()
                            && old_grid_rel_path != new_grid_rel_path
                        {
                            info!(
                                "Requesting grid file rename: '{}' -> '{}'",
                                old_grid_rel_path.display(),
                                new_grid_rel_path.display()
                            );
                            file_rename_writer.write(RequestRenameSheetFile {
                                old_relative_path: old_grid_rel_path,
                                new_relative_path: new_grid_rel_path,
                            });
                        }
                    } else {
                        info!(
                            "DB mode: Skipping grid file rename request for '{:?}/{}'",
                            category, new_name
                        );
                    }

                    if meta_to_save.category.is_none() {
                        // Request meta file rename only if old name existed and names differ
                        if !old_meta_rel_path.as_os_str().is_empty()
                            && old_meta_rel_path != new_meta_rel_path
                        {
                            info!(
                                "Requesting meta file rename: '{}' -> '{}'",
                                old_meta_rel_path.display(),
                                new_meta_rel_path.display()
                            );
                            file_rename_writer.write(RequestRenameSheetFile {
                                old_relative_path: old_meta_rel_path,
                                new_relative_path: new_meta_rel_path,
                            });
                        }
                    } else {
                        info!(
                            "DB mode: Skipping meta file rename request for '{:?}/{}'",
                            category, new_name
                        );
                    }

                    // Cascade rename for child structure sheets in the registry (any depth)
                    // We do an immutable scan first, then apply renames.
                    let old_prefix = format!("{}_", old_name);
                    let mut child_pairs: Vec<(String, String)> = Vec::new();
                    {
                        let registry_ro = registry.as_ref();
                        for (cat, name, _data) in registry_ro.iter_sheets() {
                            if *cat == *category && name.starts_with(&old_prefix) {
                                let new_key = format!("{}{}", new_name, &name[old_name.len()..]);
                                child_pairs.push((name.clone(), new_key));
                            }
                        }
                    }
                    // Apply renames in registry and emit events for cache updates
                    for (child_old, child_new) in child_pairs {
                        if child_old == child_new { continue; }
                        match registry.rename_sheet(category, &child_old, child_new.clone()) {
                            Ok(_) => {
                                info!(
                                    "Renamed child sheet in registry: '{:?}/{}' -> '{:?}/{}'",
                                    category, child_old, category, child_new
                                );
                                cache_rename_writer.write(RequestRenameCacheEntry {
                                    category: category.clone(),
                                    old_name: child_old,
                                    new_name: child_new,
                                });
                            }
                            Err(e) => warn!(
                                "Failed to rename child sheet in registry: {} -> {}: {}",
                                child_old, child_new, e
                            ),
                        }
                    }
                } else {
                    // Should not happen if rename_sheet succeeded
                    error!("Critical error: Metadata missing after successful registry rename for '{:?}/{}'. Cannot save or rename files.", category, new_name);
                    feedback_writer.write(SheetOperationFeedback {
                        message: format!(
                            "Internal error after rename for '{}'. File operations skipped.",
                            new_name
                        ),
                        is_error: true,
                    });
                }
            }
            Err(e) => {
                let msg = format!(
                    "Failed to rename sheet '{:?}/{}': {}",
                    category, old_name, e
                );
                error!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        }
    }
}

// No forwarder needed; cache listens to RequestRenameCacheEntry directly.
