// src/sheets/systems/io/save.rs

use bevy::prelude::{error, info, trace, warn, EventReader, Res, ResMut}; // Adjusted imports
use std::{
    fs::{self, File},
    io::BufWriter,
    path::Path, // Needed for Path::join
};

// Import get_default_data_base_path correctly relative to this file's module
use super::get_default_data_base_path;
use crate::sheets::{
    definitions::{SheetGridData, SheetMetadata}, // Added SheetMetadata
    events::{RequestDeleteSheetFile, RequestRenameSheetFile},
    resources::SheetRegistry,
};


/// Saves the grid data AND metadata for a single, specified sheet to their corresponding files.
/// Takes the registry (read-only) and the sheet name as arguments.
pub fn save_single_sheet(registry: &SheetRegistry, sheet_name: &str) {
    info!("Attempting to save single sheet: '{}'", sheet_name);

    match registry.get_sheet(sheet_name) {
        Some(sheet_data) => {
            let base_path = get_default_data_base_path();
            if let Err(e) = fs::create_dir_all(&base_path) {
                error!("Failed to ensure data directory '{:?}' exists for saving sheet '{}': {}. Aborting save.", base_path, sheet_name, e);
                return;
            }

            let mut grid_saved_successfully = false;

            // --- Save Grid Data ---
            if let Some(meta) = &sheet_data.metadata {
                let filename = &meta.data_filename;
                if filename.is_empty() {
                    warn!("Skipping grid save for sheet '{}': Filename in metadata is empty.", sheet_name);
                } else if meta.sheet_name != sheet_name {
                     error!(
                         "Save inconsistency: Metadata name ('{}') does not match requested save name ('{}'). Aborting grid save.",
                         meta.sheet_name, sheet_name
                     );
                } else {
                    let full_path = base_path.join(filename);
                    trace!("Saving grid for sheet '{}' to '{}'...", sheet_name, full_path.display());
                    match File::create(&full_path) {
                        Ok(file) => {
                            let writer = BufWriter::new(file);
                            match serde_json::to_writer_pretty(writer, &sheet_data.grid) {
                                Ok(_) => {
                                    info!("Successfully saved grid for sheet '{}' to '{}'.", sheet_name, full_path.display());
                                    grid_saved_successfully = true; // Mark grid as saved
                                }
                                Err(e) => error!("Failed to serialize grid for sheet '{}' to '{}': {}", sheet_name, full_path.display(), e),
                            }
                        }
                        Err(e) => error!("Failed to create/open grid file '{}' for sheet '{}': {}", full_path.display(), sheet_name, e),
                    }
                }
            } else {
                warn!("Skipping grid save for sheet '{}': Metadata missing.", sheet_name);
            }

            // --- Save Metadata ---
            if let Some(meta) = &sheet_data.metadata {
                 // Derive metadata filename from the sheet name in metadata (which should match sheet_name)
                 let meta_filename = format!("{}.meta.json", meta.sheet_name);
                 let meta_path = base_path.join(&meta_filename);

                 // Sanity check again before saving metadata file
                 if meta.sheet_name != sheet_name {
                     error!(
                         "Save inconsistency: Metadata name ('{}') does not match requested save name ('{}'). Aborting metadata save.",
                         meta.sheet_name, sheet_name
                     );
                 } else {
                     trace!("Saving metadata for sheet '{}' to '{}'...", sheet_name, meta_path.display());
                     match File::create(&meta_path) {
                         Ok(file) => {
                             let writer = BufWriter::new(file);
                             match serde_json::to_writer_pretty(writer, meta) { // Serialize the whole SheetMetadata
                                 Ok(_) => {
                                     info!("Successfully saved metadata for sheet '{}' to '{}'.", sheet_name, meta_path.display());
                                     // Metadata saved successfully
                                 }
                                 Err(e) => {
                                     error!("Failed to serialize metadata for sheet '{}' to '{}': {}", sheet_name, meta_path.display(), e);
                                 }
                             }
                         }
                         Err(e) => {
                             error!("Failed to create/open metadata file '{}' for sheet '{}': {}", meta_path.display(), sheet_name, e);
                         }
                     }
                 } // End metadata sanity check else
            } else if grid_saved_successfully { // Only warn if grid was saved but metadata is missing
                 warn!("Grid data saved for sheet '{}', but metadata was missing, so no metadata file saved.", sheet_name);
            }
            // No explicit warning needed if neither grid nor metadata could be saved (prior errors logged).

        } // End Some(sheet_data)
        None => {
            error!("Failed to save sheet '{}': Sheet not found in registry.", sheet_name);
        }
    }
}


// --- save_all_sheets_logic is now potentially unused ---
// You can keep it if you might add a "Save All" button later,
// or remove it entirely if only single-sheet saving is desired.

/// Core logic function to save ALL registered sheets to JSON files.
#[allow(dead_code)] // Mark as unused for now if keeping it
pub fn save_all_sheets_logic(registry: &SheetRegistry) {
    if registry.get_sheet_names().is_empty() {
        trace!("Save All skipped: No sheets in registry.");
        return;
    }
    info!("Attempting to save ALL sheets...");
    let base_path = get_default_data_base_path();

    if let Err(e) = fs::create_dir_all(&base_path) {
        error!("Save All: Failed to create data directory '{:?}' for saving: {}. Aborting save.", base_path, e);
        return;
    }
    let mut saved_count = 0;
    let mut error_count = 0;

    // Iterate using sheet names to call the single save function
    for sheet_name in registry.get_sheet_names() {
         // Call the single sheet save logic which now handles both grid and meta
         // Note: save_single_sheet logs its own errors/successes.
         // We could potentially capture success/failure here if needed for a summary.
         save_single_sheet(registry, sheet_name);
         // For simplicity, we won't track counts meticulously here as save_single_sheet logs.
         // If detailed summary is needed, save_single_sheet would need to return status.
    }

    info!("Finished Save All attempt. Check logs for details of individual sheets.");
    // Simplified logging for Save All completion
}


// --- File Operation Handlers Remain the Same ---

/// Handles the `RequestDeleteSheetFile` event. (Remains)
pub fn handle_delete_sheet_file_request(
    mut events: EventReader<RequestDeleteSheetFile>,
) {
    let base_path = get_default_data_base_path();
    for event in events.read() {
        if event.filename.is_empty() { warn!("Skipping file deletion request: filename is empty."); continue; }
        let full_path = base_path.join(&event.filename);
        info!("Handling request to delete file: '{}'", full_path.display());
        if full_path.exists() {
            match fs::remove_file(&full_path) {
                Ok(_) => info!("Successfully deleted file: '{}'", full_path.display()),
                Err(e) => error!("Failed to delete file '{}': {}", full_path.display(), e),
            }
        } else {
            // It's not necessarily an error if the file doesn't exist (e.g., deleted manually)
            // Change level to info or trace depending on expected behavior
             info!("File '{}' not found for deletion request (might have been deleted already).", full_path.display());
        }
    }
}

/// Handles the `RequestRenameSheetFile` event. (Remains)
pub fn handle_rename_sheet_file_request(
     mut events: EventReader<RequestRenameSheetFile>,
) {
     let base_path = get_default_data_base_path();
     for event in events.read() {
         if event.old_filename.is_empty() || event.new_filename.is_empty() { warn!("Skipping file rename request: old or new filename is empty."); continue; }
         if event.old_filename == event.new_filename { warn!("Skipping file rename request: old and new filenames are the same ('{}').", event.old_filename); continue; }
         let old_path = base_path.join(&event.old_filename);
         let new_path = base_path.join(&event.new_filename);
         info!("Handling request to rename file: '{}' -> '{}'", old_path.display(), new_path.display());
         if !old_path.exists() { warn!("Cannot rename: Old file '{}' not found.", old_path.display()); continue; }
         if new_path.exists() { error!("Cannot rename: Target file '{}' already exists.", new_path.display()); continue; }
         match fs::rename(&old_path, &new_path) {
             Ok(_) => info!("Successfully renamed file: '{}' -> '{}'", old_path.display(), new_path.display()),
             Err(e) => error!("Failed to rename file '{}' to '{}': {}", old_path.display(), new_path.display(), e),
         }
     }
}

// --- Autosave Systems REMOVED ---