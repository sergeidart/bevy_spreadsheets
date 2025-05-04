// src/sheets/systems/logic.rs
use bevy::prelude::*;
use crate::sheets::systems::io::save::save_single_sheet;
use crate::sheets::{
    events::{
        AddSheetRowRequest, RequestRenameSheet, RequestDeleteSheet,
        RequestDeleteSheetFile, RequestRenameSheetFile,
        SheetOperationFeedback,
        RequestUpdateColumnName, // Added Event
    },
    resources::SheetRegistry,
};

pub fn handle_add_row_request(
    mut events: EventReader<AddSheetRowRequest>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    for event in events.read() {
        let sheet_name = &event.sheet_name;
        if let Some(sheet_data) = registry.get_sheet_mut(sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_cols = metadata.column_headers.len();
                sheet_data.grid.push(vec![String::new(); num_cols]);
                let msg = format!("Added row to sheet '{}'.", sheet_name);
                info!("{}", msg); // Keep internal log
                feedback_writer.send(SheetOperationFeedback { // Send user feedback
                     message: msg,
                     is_error: false,
                 });
                info!("Row added to '{}', triggering immediate save.", sheet_name);
                save_single_sheet(&registry, sheet_name); // MODIFIED CALL
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
        // Perform rename within a scope to manage mutable borrow
        let rename_result = registry.rename_sheet(old_name, new_name.clone());

        match rename_result {
            Ok(moved_data) => {
                let success_msg = format!("Successfully renamed sheet '{}' to '{}' in registry.", old_name, new_name);
                info!("{}", success_msg);
                feedback_writer.send(SheetOperationFeedback { message: success_msg, is_error: false });

                // --- Trigger Save for the NEW sheet name ---
                info!("Registry renamed to '{}', triggering immediate save.", new_name);
                save_single_sheet(&registry, new_name); // Call save with immutable borrow

                // --- Trigger File Rename AFTER saving the new state ---
                if let Some(old_filename) = old_filename_opt {
                    // Use filename from the moved data (which should be updated)
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
        let metadata_opt = match registry.get_sheet(sheet_name) {
            Some(sheet_data) => sheet_data.metadata.clone(), // Clone metadata if exists
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

                // Request file deletion if a filename existed in the original metadata
                if let Some(metadata) = metadata_opt {
                     let filename_to_delete = &metadata.data_filename;
                     let meta_filename_to_delete = format!("{}.meta.json", metadata.sheet_name); // Get meta filename

                     if !filename_to_delete.is_empty() {
                         file_delete_writer.send(RequestDeleteSheetFile { filename: filename_to_delete.clone() });
                         info!("Requested deletion of associated grid file: '{}'", filename_to_delete);
                     }
                     // Always request deletion of the metadata file (even if grid filename was empty)
                     file_delete_writer.send(RequestDeleteSheetFile { filename: meta_filename_to_delete.clone() });
                     info!("Requested deletion of associated metadata file: '{}'", meta_filename_to_delete);
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

pub fn handle_update_column_name(
    mut events: EventReader<RequestUpdateColumnName>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    let mut changed_sheets = Vec::new(); // Track sheets that changed to save later

    for event in events.read() {
        let sheet_name = &event.sheet_name;
        let col_index = event.column_index;
        let new_name = event.new_name.trim(); // Trim whitespace

        if new_name.is_empty() {
            let msg = format!("Failed to rename column {} for sheet '{}': New name cannot be empty.", col_index + 1, sheet_name);
            warn!("{}", msg);
            feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            continue;
        }

        let mut success = false;
        if let Some(sheet_data) = registry.get_sheet_mut(sheet_name) {
            if let Some(metadata) = &mut sheet_data.metadata {
                if col_index < metadata.column_headers.len() {
                    // Check if new name conflicts with existing names in the same sheet (case-insensitive check might be better)
                    if metadata.column_headers.iter().any(|h| h.eq_ignore_ascii_case(new_name)) {
                        let msg = format!("Failed to rename column for sheet '{}': Name '{}' already exists (ignoring case).", sheet_name, new_name);
                        warn!("{}", msg);
                        feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
                        continue; // Skip this event
                    }

                    let old_name = std::mem::replace(&mut metadata.column_headers[col_index], new_name.to_string());
                    let msg = format!("Renamed column {} from '{}' to '{}' for sheet '{}'.", col_index + 1, old_name, new_name, sheet_name);
                    info!("{}", msg);
                    feedback_writer.send(SheetOperationFeedback { message: msg, is_error: false });
                    success = true; // Mark as success

                } else {
                    let msg = format!("Failed to rename column for sheet '{}': Index {} out of bounds ({} columns).", sheet_name, col_index, metadata.column_headers.len());
                    error!("{}", msg);
                    feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
                }
            } else {
                let msg = format!("Failed to rename column for sheet '{}': Metadata missing.", sheet_name);
                warn!("{}", msg);
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            }
        } else {
            let msg = format!("Failed to rename column: Sheet '{}' not found.", sheet_name);
            warn!("{}", msg);
            feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
        }

        // If successful, mark the sheet for saving
        if success {
             if !changed_sheets.contains(sheet_name) {
                 changed_sheets.push(sheet_name.clone());
             }
        }
    }

    // Trigger save for all affected sheets after processing events
    for sheet_name in changed_sheets {
         info!("Column name updated for '{}', triggering immediate save.", sheet_name);
         save_single_sheet(&registry, &sheet_name);
    }
}