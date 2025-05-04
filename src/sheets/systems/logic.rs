// src/sheets/systems/logic.rs
use bevy::prelude::*;
// ADDED IMPORT for save function
use crate::sheets::systems::io::save::save_single_sheet; // CHANGED IMPORT

use crate::sheets::{
    // definitions::SheetGridData, // Keep if needed
    events::{
        AddSheetRowRequest, RequestRenameSheet, RequestDeleteSheet,
        RequestDeleteSheetFile, RequestRenameSheetFile,
        SheetOperationFeedback,
    },
    resources::SheetRegistry,
};

/// Handles the `AddSheetRowRequest` event sent from the UI. Triggers save on success.
pub fn handle_add_row_request(
    mut events: EventReader<AddSheetRowRequest>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    // No need for 'changed' flag, save inside the loop
    for event in events.read() {
        let sheet_name = &event.sheet_name;
        if let Some(sheet_data) = registry.get_sheet_mut(sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_cols = metadata.column_headers.len();
                if num_cols > 0 {
                    sheet_data.grid.push(vec![String::new(); num_cols]);
                    let msg = format!("Added row to sheet '{}'.", sheet_name);
                    info!("{}", msg); // Keep internal log
                    feedback_writer.send(SheetOperationFeedback { // Send user feedback
                         message: msg,
                         is_error: false,
                     });
                    // Save the specific sheet immediately
                    info!("Row added to '{}', triggering immediate save.", sheet_name);
                    save_single_sheet(&registry, sheet_name); // MODIFIED CALL
                } else {
                    let msg = format!("Cannot add row to sheet '{}': No columns defined in metadata.", sheet_name);
                    warn!("{}", msg);
                    feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
                }
            } else {
                 let msg = format!("Cannot add row to sheet '{}': Metadata missing.", sheet_name);
                 warn!("{}", msg);
                 feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            }
        } else {
            let msg = format!("Cannot add row: Sheet '{}' not found in registry.", sheet_name);
            warn!("{}", msg);
            feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
        }
    }
}


/// Handles the `RequestRenameSheet` event. Triggers save for the new name on success.
pub fn handle_rename_request(
    mut events: EventReader<RequestRenameSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut file_rename_writer: EventWriter<RequestRenameSheetFile>,
) {
    // Process each rename event individually
    for event in events.read() {
        let old_name = &event.old_name;
        let new_name = &event.new_name; // Clone only when needed
        info!("Handling rename request: '{}' -> '{}'", old_name, new_name);

        // Capture old filename BEFORE potential registry rename
        let old_filename_opt: Option<String> = registry
            .get_sheet(old_name)
            .and_then(|data| data.metadata.as_ref())
            .map(|meta| meta.data_filename.clone());

        // --- VALIDATION ---
        if new_name.trim().is_empty() {
            let msg = "Rename failed: New name cannot be empty.".to_string();
            error!("{}", msg);
            feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            continue; // Process next event
        }
        if old_name == new_name {
             let msg = "Rename failed: New name is the same as the old name.".to_string();
             warn!("{}", msg);
             feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
             continue; // Process next event
        }
        if registry.get_sheet(new_name).is_some() {
            let msg = format!("Rename failed: A sheet named '{}' already exists.", new_name);
            error!("{}", msg);
            feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            continue; // Process next event
        }

        // --- EXECUTION (Call registry rename) ---
        match registry.rename_sheet(old_name, new_name.clone()) {
            Ok(moved_data) => {
                let success_msg = format!("Successfully renamed sheet '{}' to '{}' in registry.", old_name, new_name);
                info!("{}", success_msg);
                feedback_writer.send(SheetOperationFeedback { message: success_msg, is_error: false });

                // --- Trigger Save for the NEW sheet name ---
                info!("Registry renamed to '{}', triggering immediate save.", new_name);
                save_single_sheet(&registry, new_name); // MODIFIED CALL (save new name)

                // --- Trigger File Rename AFTER saving the new state ---
                if let Some(old_filename) = old_filename_opt {
                    let new_filename_opt = moved_data.metadata.map(|meta| meta.data_filename);
                    if let Some(new_filename) = new_filename_opt {
                        if !old_filename.is_empty() && !new_filename.is_empty() && old_filename != new_filename {
                             info!("Requesting file rename: '{}' -> '{}'", old_filename, new_filename);
                             file_rename_writer.send(RequestRenameSheetFile { old_filename, new_filename });
                        } else {
                           trace!("Skipping file rename for sheet '{}': old='{}', new='{}'. No change needed or invalid names.", new_name, old_filename, new_filename);
                        }
                    } else {
                         warn!("Could not get new filename from metadata after renaming sheet '{}'. Skipping file rename.", new_name);
                    }
                } else {
                    info!("Skipping file rename for sheet '{}': Original sheet had no associated filename.", new_name);
                }
            }
            Err(e) => {
                let msg = format!("Failed to rename sheet '{}': {}", old_name, e);
                error!("{}", msg);
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            }
        }
    } // End event loop
}

/// Handles the `RequestDeleteSheet` event: Validates and performs deletion. No save needed.
pub fn handle_delete_request(
    mut events: EventReader<RequestDeleteSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut file_delete_writer: EventWriter<RequestDeleteSheetFile>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    // No save needed here. File deletion is handled separately.
    for event in events.read() {
        let sheet_name = &event.sheet_name;
        info!("Handling delete request for sheet: '{}'", sheet_name);

        // --- VALIDATION (Check existence before getting filename) ---
        let filename_to_delete = match registry.get_sheet(sheet_name) {
            Some(sheet_data) => {
                 sheet_data.metadata.as_ref().map(|m| m.data_filename.clone())
            }
            None => {
                 let msg = format!("Delete failed: Sheet '{}' not found.", sheet_name);
                 error!("{}", msg);
                 feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
                 continue; // Skip to next event
            }
        };

        // --- EXECUTION ---
        match registry.delete_sheet(sheet_name) {
            Ok(_removed_data) => {
                let msg = format!("Successfully deleted sheet '{}'.", sheet_name);
                info!("{}", msg); // Internal log
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: false });

                // Request file deletion if a filename existed
                if let Some(filename) = filename_to_delete {
                    if !filename.is_empty() {
                        file_delete_writer.send(RequestDeleteSheetFile { filename: filename.clone() });
                        info!("Requested deletion of associated file: '{}'", filename);
                    }
                }
            }
            Err(e) => {
                // This case should be less likely now due to the pre-check
                let msg = format!("Failed to delete sheet '{}' from registry: {}", sheet_name, e);
                error!("{}", msg);
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            }
        }
    }
}