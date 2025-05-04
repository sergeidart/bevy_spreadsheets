// src/sheets/systems/logic/rename_sheet.rs
use bevy::prelude::*;
use crate::sheets::{
    events::{RequestRenameSheet, RequestRenameSheetFile, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};

/// Handles requests to rename a sheet, updating the registry and related filenames.
pub fn handle_rename_request(
    mut events: EventReader<RequestRenameSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut file_rename_writer: EventWriter<RequestRenameSheetFile>,
) {
    for event in events.read() {
        let old_name = &event.old_name;
        let new_name = &event.new_name;
        info!("Handling rename request: '{}' -> '{}'", old_name, new_name);

        // --- Validation ---
        if new_name.trim().is_empty() {
            feedback_writer.send(SheetOperationFeedback { message: "Rename failed: New name cannot be empty.".to_string(), is_error: true });
            continue; // Skip to next event
        }
        if old_name == new_name {
            feedback_writer.send(SheetOperationFeedback { message: "Rename failed: New name is the same as the old name.".to_string(), is_error: true });
            continue;
        }
        // Check existence using immutable borrow before attempting mutable borrow
        if registry.get_sheet(new_name).is_some() {
            feedback_writer.send(SheetOperationFeedback { message: format!("Rename failed: A sheet named '{}' already exists.", new_name), is_error: true });
            continue;
        }

        // --- Get old filenames BEFORE attempting rename ---
        // This requires an immutable borrow first
        let (old_grid_filename_opt, old_meta_filename_opt) = {
            let registry_immut = registry.as_ref(); // Immutable borrow
            let grid_fn = registry_immut.get_sheet(old_name)
                .and_then(|d| d.metadata.as_ref())
                .map(|m| m.data_filename.clone());
            let meta_fn = registry_immut.get_sheet(old_name)
                .map(|_| format!("{}.meta.json", old_name));
            (grid_fn, meta_fn)
        };

        // --- Perform Rename in Registry (Mutable Borrow) ---
        let rename_result = registry.rename_sheet(old_name, new_name.clone());

        match rename_result {
            Ok(moved_data) => {
                let success_msg = format!("Successfully renamed sheet '{}' to '{}'.", old_name, new_name);
                info!("{}", success_msg);
                feedback_writer.send(SheetOperationFeedback { message: success_msg, is_error: false });

                // Trigger save of the newly renamed sheet
                // Need immutable borrow again for save
                let registry_immut = registry.as_ref();
                save_single_sheet(registry_immut, new_name);

                // --- Request File Renames ---
                let new_grid_filename = moved_data.metadata.as_ref().map(|m| m.data_filename.clone());
                let new_meta_filename = format!("{}.meta.json", new_name);

                // Rename grid data file if necessary
                if let Some(old_grid_filename) = old_grid_filename_opt {
                    if let Some(new_grid_filename_val) = new_grid_filename {
                         if !old_grid_filename.is_empty() && !new_grid_filename_val.is_empty() && old_grid_filename != new_grid_filename_val {
                             info!("Requesting grid file rename: '{}' -> '{}'", old_grid_filename, new_grid_filename_val);
                             file_rename_writer.send(RequestRenameSheetFile { old_filename: old_grid_filename, new_filename: new_grid_filename_val });
                         }
                    } else {
                        warn!("New grid filename missing after successful rename of sheet '{}'", new_name);
                    }
                }

                // Rename metadata file if necessary
                if let Some(old_meta_filename) = old_meta_filename_opt {
                     if old_meta_filename != new_meta_filename {
                          info!("Requesting meta file rename: '{}' -> '{}'", old_meta_filename, new_meta_filename);
                          file_rename_writer.send(RequestRenameSheetFile { old_filename: old_meta_filename, new_filename: new_meta_filename });
                     }
                 } else {
                      warn!("Old meta filename missing for sheet '{}' during rename.", old_name);
                 }
            }
            Err(e) => {
                let msg = format!("Failed to rename sheet '{}': {}", old_name, e);
                error!("{}", msg);
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            }
        }
    }
}