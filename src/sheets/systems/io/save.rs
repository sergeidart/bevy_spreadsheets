// src/sheets/systems/io/save.rs

use bevy::prelude::{error, info, trace, warn, EventReader};
use std::{
    fs::{self, File},
    io::BufWriter,
};

// Corrected imports relative to this file's module position
use super::{get_default_data_base_path, get_full_metadata_path, get_full_sheet_path};
use crate::sheets::{
    definitions::SheetMetadata, // Added SheetMetadata
    events::{RequestDeleteSheetFile, RequestRenameSheetFile},
    resources::SheetRegistry,
};

/// Saves the grid data AND metadata for a single sheet using its metadata for path info.
/// Takes the registry (read-only) and the SheetMetadata of the sheet to save.
pub fn save_single_sheet(registry: &SheetRegistry, metadata_to_save: &SheetMetadata) {
    let sheet_name = &metadata_to_save.sheet_name;
    let category = &metadata_to_save.category;
    // If a category is set, we treat this as a database-backed sheet and skip JSON persistence.
    // This prevents re-spawning JSON files when using the DB storage path.
    if category.is_some() {
        trace!(
            "Skipping JSON save for DB-backed sheet '{:?}/{}'",
            category,
            sheet_name
        );
        return;
    }
    // Skip saving virtual (ephemeral) sheets
    if sheet_name.starts_with("__virtual__") {
        trace!(
            "Skipping save for ephemeral virtual sheet '{:?}/{}'",
            category,
            sheet_name
        );
        return;
    }
    info!("Attempting to save sheet: '{:?}/{}'", category, sheet_name);

    // Get the actual SheetGridData from the registry using category and name
    match registry.get_sheet(category, sheet_name) {
        Some(sheet_data) => {
            let base_path = get_default_data_base_path();
            let category_path = if let Some(cat_name) = category {
                base_path.join(cat_name)
            } else {
                base_path.clone() // Save to root data_sheets dir
            };

            // --- Ensure Category Directory Exists ---
            if let Err(e) = fs::create_dir_all(&category_path) {
                error!("Failed to ensure category directory '{:?}' exists for saving sheet '{}': {}. Aborting save.", category_path, sheet_name, e);
                return;
            }

            let mut grid_saved_successfully = false;

            // --- Save Grid Data ---
            // Use the helper function to get the full path
            let grid_full_path = get_full_sheet_path(&base_path, metadata_to_save);
            trace!(
                "Saving grid for sheet '{:?}/{}' to '{}'...",
                category,
                sheet_name,
                grid_full_path.display()
            );

            match File::create(&grid_full_path) {
                Ok(file) => {
                    let writer = BufWriter::new(file);
                    match serde_json::to_writer_pretty(writer, &sheet_data.grid) {
                        Ok(_) => {
                            info!(
                                "Successfully saved grid for sheet '{:?}/{}' to '{}'.",
                                category,
                                sheet_name,
                                grid_full_path.display()
                            );
                            grid_saved_successfully = true; // Mark grid as saved
                        }
                        Err(e) => error!(
                            "Failed to serialize grid for sheet '{:?}/{}' to '{}': {}",
                            category,
                            sheet_name,
                            grid_full_path.display(),
                            e
                        ),
                    }
                }
                Err(e) => error!(
                    "Failed to create/open grid file '{}' for sheet '{:?}/{}': {}",
                    grid_full_path.display(),
                    category,
                    sheet_name,
                    e
                ),
            }

            // --- Save Metadata ---
            // Use the helper function to get the full path
            let meta_path = get_full_metadata_path(&base_path, metadata_to_save);
            trace!(
                "Saving metadata for sheet '{:?}/{}' to '{}'...",
                category,
                sheet_name,
                meta_path.display()
            );

            match File::create(&meta_path) {
                Ok(file) => {
                    let writer = BufWriter::new(file);
                    // Serialize the *provided* metadata_to_save, which might have corrections
                    match serde_json::to_writer_pretty(writer, metadata_to_save) {
                        Ok(_) => {
                            info!(
                                "Successfully saved metadata for sheet '{:?}/{}' to '{}'.",
                                category,
                                sheet_name,
                                meta_path.display()
                            );
                            if let Some(rp) = &metadata_to_save.random_picker {
                                trace!("Save: RandomPicker mode={:?} weights={} exps={} mults={} summarizers={}", rp.mode, rp.weight_columns.len(), rp.weight_exponents.len(), rp.weight_multipliers.len(), rp.summarizer_columns.len());
                            }
                            // Metadata saved successfully
                        }
                        Err(e) => {
                            error!(
                                "Failed to serialize metadata for sheet '{:?}/{}' to '{}': {}",
                                category,
                                sheet_name,
                                meta_path.display(),
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to create/open metadata file '{}' for sheet '{:?}/{}': {}",
                        meta_path.display(),
                        category,
                        sheet_name,
                        e
                    );
                }
            }

            if !grid_saved_successfully && !sheet_data.grid.is_empty() {
                warn!("Grid data NOT saved for sheet '{:?}/{}', but metadata file might have been saved.", category, sheet_name);
            }
        } // End Some(sheet_data)
        None => {
            error!(
                "Failed to save sheet '{:?}/{}': Sheet not found in registry.",
                category, sheet_name
            );
        }
    }
}

// --- File Operation Handlers ---

/// Handles the `RequestDeleteSheetFile` event. Expects relative path (e.g., "Cat/File.json").
pub fn handle_delete_sheet_file_request(mut events: EventReader<RequestDeleteSheetFile>) {
    let base_path = get_default_data_base_path();
    for event in events.read() {
        if event.relative_path.as_os_str().is_empty() {
            warn!("Skipping file deletion request: relative path is empty.");
            continue;
        }
        // Path provided in event should be relative to data_sheets (e.g., "MyCategory/Sheet1.json")
        let full_path = base_path.join(&event.relative_path);
        info!("Handling request to delete file: '{}'", full_path.display());
        if full_path.exists() {
            match fs::remove_file(&full_path) {
                Ok(_) => info!("Successfully deleted file: '{}'", full_path.display()),
                Err(e) => error!("Failed to delete file '{}': {}", full_path.display(), e),
            }
        } else {
            info!(
                "File '{}' not found for deletion request (might have been deleted already).",
                full_path.display()
            );
        }

        // --- Attempt to delete parent directory if empty ---
        if let Some(parent_dir) = full_path.parent() {
            // Only try deleting if it's not the base data path itself
            if parent_dir != base_path {
                match fs::read_dir(parent_dir) {
                    Ok(mut read_dir) => {
                        if read_dir.next().is_none() {
                            // Directory is empty
                            info!(
                                "Attempting to remove empty parent directory: '{}'",
                                parent_dir.display()
                            );
                            match fs::remove_dir(parent_dir) {
                                   Ok(_) => info!("Successfully removed empty directory: '{}'", parent_dir.display()),
                                   Err(e) => warn!("Failed to remove directory '{}' (it might not be empty or permissions issue): {}", parent_dir.display(), e),
                              }
                        }
                    }
                    Err(e) => {
                        // Don't error if reading dir fails, just log
                        trace!(
                            "Could not read parent directory '{}' to check for emptiness: {}",
                            parent_dir.display(),
                            e
                        );
                    }
                }
            }
        }
    }
}

/// Handles the `RequestRenameSheetFile` event. Expects relative paths.
pub fn handle_rename_sheet_file_request(mut events: EventReader<RequestRenameSheetFile>) {
    let base_path = get_default_data_base_path();
    for event in events.read() {
        if event.old_relative_path.as_os_str().is_empty()
            || event.new_relative_path.as_os_str().is_empty()
        {
            warn!("Skipping file rename request: old or new relative path is empty.");
            continue;
        }
        if event.old_relative_path == event.new_relative_path {
            warn!(
                "Skipping file rename request: old and new relative paths are the same ('{}').",
                event.old_relative_path.display()
            );
            continue;
        }

        let old_path = base_path.join(&event.old_relative_path);
        let new_path = base_path.join(&event.new_relative_path);

        info!(
            "Handling request to rename file: '{}' -> '{}'",
            old_path.display(),
            new_path.display()
        );

        if !old_path.exists() {
            warn!(
                "Cannot rename: Old file '{}' not found.",
                old_path.display()
            );
            continue;
        }
        if new_path.exists() {
            error!(
                "Cannot rename: Target file '{}' already exists.",
                new_path.display()
            );
            continue;
        }

        // Ensure target directory exists (needed if category changes, though rename currently doesn't support that)
        if let Some(new_parent) = new_path.parent() {
            if !new_parent.exists() {
                if let Err(e) = fs::create_dir_all(new_parent) {
                    error!(
                        "Cannot rename: Failed to create target directory '{}': {}",
                        new_parent.display(),
                        e
                    );
                    continue;
                }
            }
        } else {
            error!(
                "Cannot rename: Could not determine parent directory for new path '{}'.",
                new_path.display()
            );
            continue;
        }

        match fs::rename(&old_path, &new_path) {
            Ok(_) => info!(
                "Successfully renamed file: '{}' -> '{}'",
                old_path.display(),
                new_path.display()
            ),
            Err(e) => error!(
                "Failed to rename file '{}' to '{}': {}",
                old_path.display(),
                new_path.display(),
                e
            ),
        }

        // --- Attempt to delete old parent directory if empty ---
        if let Some(old_parent_dir) = old_path.parent() {
            // Only try deleting if it's not the base data path itself and differs from new parent
            if old_parent_dir != base_path
                && old_parent_dir != new_path.parent().unwrap_or(old_parent_dir)
            {
                match fs::read_dir(old_parent_dir) {
                    Ok(mut read_dir) => {
                        if read_dir.next().is_none() {
                            // Directory is empty
                            info!(
                                "Attempting to remove empty old parent directory: '{}'",
                                old_parent_dir.display()
                            );
                            match fs::remove_dir(old_parent_dir) {
                                Ok(_) => info!(
                                    "Successfully removed empty old directory: '{}'",
                                    old_parent_dir.display()
                                ),
                                Err(e) => warn!(
                                    "Failed to remove old directory '{}': {}",
                                    old_parent_dir.display(),
                                    e
                                ),
                            }
                        }
                    }
                    Err(e) => {
                        trace!(
                            "Could not read old parent directory '{}' to check for emptiness: {}",
                            old_parent_dir.display(),
                            e
                        );
                    }
                }
            }
        }
    }
}
