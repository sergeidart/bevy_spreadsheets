// src/sheets/systems/io/save.rs

use bevy::prelude::{error, info, trace, warn, EventReader}; // Adjusted imports
use std::{
    fs::{self, File},
    io::BufWriter,
    path::Path, // Needed for Path::join
};

// Import get_default_data_base_path correctly relative to this file's module
use super::get_default_data_base_path;
use crate::sheets::{
    definitions::SheetGridData, // Needed for save_single_sheet
    events::{RequestDeleteSheetFile, RequestRenameSheetFile},
    resources::SheetRegistry,
};


/// Saves the grid data for a single, specified sheet to its corresponding file.
/// Takes the registry (read-only) and the sheet name as arguments.
pub fn save_single_sheet(registry: &SheetRegistry, sheet_name: &str) {
    info!("Attempting to save single sheet: '{}'", sheet_name);

    // 1. Get Sheet Data
    match registry.get_sheet(sheet_name) {
        Some(sheet_data) => {
            // 2. Get Metadata and Filename
            if let Some(meta) = &sheet_data.metadata {
                let filename = &meta.data_filename;
                if filename.is_empty() {
                    warn!("Skipping save for sheet '{}': Filename in metadata is empty.", sheet_name);
                    return;
                }

                // Ensure sheet name in metadata matches (sanity check)
                if meta.sheet_name != sheet_name {
                     error!(
                         "Save inconsistency: Metadata name ('{}') does not match requested save name ('{}'). Aborting save.",
                         meta.sheet_name, sheet_name
                    );
                     return;
                }

                // 3. Prepare File Path
                let base_path = get_default_data_base_path();
                if let Err(e) = fs::create_dir_all(&base_path) {
                    error!("Failed to create data directory '{:?}' for saving sheet '{}': {}. Aborting save.", base_path, sheet_name, e);
                    return;
                }
                let full_path = base_path.join(filename);
                trace!("Saving sheet '{}' to '{}'...", sheet_name, full_path.display());

                // 4. Write to File
                match File::create(&full_path) {
                    Ok(file) => {
                        let writer = BufWriter::new(file);
                        match serde_json::to_writer_pretty(writer, &sheet_data.grid) {
                            Ok(_) => {
                                info!("Successfully saved sheet '{}' to '{}'.", sheet_name, full_path.display());
                            }
                            Err(e) => {
                                error!("Failed to serialize sheet '{}' to JSON file '{}': {}", sheet_name, full_path.display(), e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to create/open file '{}' for sheet '{}': {}", full_path.display(), sheet_name, e);
                    }
                }
            } else {
                warn!("Skipping save for sheet '{}': Metadata missing.", sheet_name);
            }
        }
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

    for (sheet_name, sheet_data) in registry.iter_sheets() {
        // Reusing single save logic might be cleaner, but for now, duplicate:
        if let Some(meta) = &sheet_data.metadata {
            let filename = &meta.data_filename;
            if filename.is_empty() {
                 warn!("Save All: Skipping sheet '{}' due to empty filename.", sheet_name);
                 continue;
            }
            let full_path = base_path.join(filename);
            trace!("Save All: Saving sheet '{}' to '{}'...", sheet_name, full_path.display());
            match File::create(&full_path) {
                Ok(file) => {
                    let writer = BufWriter::new(file);
                    match serde_json::to_writer_pretty(writer, &sheet_data.grid) {
                        Ok(_) => saved_count += 1,
                        Err(e) => {
                            error!("Save All: Failed to serialize sheet '{}' to JSON: {}", sheet_name, e);
                            error_count += 1;
                        }
                    }
                }
                Err(e) => {
                    error!("Save All: Failed to create/open file '{}' for sheet '{}': {}", full_path.display(), sheet_name, e);
                    error_count += 1;
                }
            }
        } else {
            warn!("Save All: Skipping sheet '{}': No metadata.", sheet_name);
        }
    }

    if error_count > 0 {
         error!("Finished Save All with {} errors.", error_count);
    } else if saved_count > 0 {
        info!("Successfully saved {} sheets via Save All.", saved_count);
    } else {
        trace!("Save All: No sheets requiring saving were processed.");
    }
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
        } else { warn!("File '{}' not found for deletion.", full_path.display()); }
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