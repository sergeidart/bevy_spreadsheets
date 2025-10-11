// src/sheets/systems/logic/rename_sheet.rs
use crate::sheets::{
    events::{RequestRenameSheet, RequestRenameSheetFile, SheetOperationFeedback},
    resources::SheetRegistry,
};
use bevy::prelude::*;
use std::path::PathBuf; // Added for relative path

/// Handles requests to rename a sheet *within its category*.
pub fn handle_rename_request(
    mut events: EventReader<RequestRenameSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut file_rename_writer: EventWriter<RequestRenameSheetFile>,
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
                    // In DB mode, there are no filesystem files to rename or save.
                    if meta_to_save.category.is_some() {
                        info!(
                            "DB mode: Skipping JSON file save for renamed sheet '{:?}/{}'",
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
