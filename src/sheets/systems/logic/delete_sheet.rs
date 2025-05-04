// src/sheets/systems/logic/delete_sheet.rs
use bevy::prelude::*;
use crate::sheets::{
    events::{RequestDeleteSheet, RequestDeleteSheetFile, SheetOperationFeedback},
    resources::SheetRegistry,
};

/// Handles requests to delete a sheet from the registry and requests deletion of associated files.
pub fn handle_delete_request(
    mut events: EventReader<RequestDeleteSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut file_delete_writer: EventWriter<RequestDeleteSheetFile>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    for event in events.read() {
        let sheet_name = &event.sheet_name;
        info!("Handling delete request for sheet: '{}'", sheet_name);

        // --- Get metadata BEFORE attempting delete ---
        // Need immutable borrow first
        let metadata_opt = {
            let registry_immut = registry.as_ref();
            registry_immut.get_sheet(sheet_name).and_then(|d| d.metadata.clone()) // Clone metadata if present
        };

        // Check if sheet exists before attempting delete (using immutable borrow again)
        if registry.get_sheet(sheet_name).is_none() {
             let msg = format!("Delete failed: Sheet '{}' not found.", sheet_name);
             error!("{}", msg);
             feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
             continue; // Skip to next event
        }
        if metadata_opt.is_none() && registry.get_sheet(sheet_name).is_some() {
            warn!("Sheet '{}' exists but metadata is missing during delete request processing.", sheet_name);
            // Proceed with registry deletion, but file deletion might be incomplete.
        }

        // --- Perform Delete in Registry (Mutable Borrow) ---
        match registry.delete_sheet(sheet_name) {
            Ok(_) => {
                let msg = format!("Successfully deleted sheet '{}' from registry.", sheet_name);
                info!("{}", msg);
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: false });

                // --- Request File Deletions (using cloned metadata) ---
                if let Some(metadata) = metadata_opt {
                     let filename_to_delete = &metadata.data_filename;
                     // Use sheet_name from metadata for consistency, though it should match event.sheet_name
                     let meta_filename_to_delete = format!("{}.meta.json", metadata.sheet_name);

                     if !filename_to_delete.is_empty() {
                         info!("Requesting grid file deletion: '{}'", filename_to_delete);
                         file_delete_writer.send(RequestDeleteSheetFile { filename: filename_to_delete.clone() });
                     } else {
                         warn!("No grid filename found in metadata for deleted sheet '{}'.", metadata.sheet_name);
                     }
                     info!("Requesting meta file deletion: '{}'", meta_filename_to_delete);
                     file_delete_writer.send(RequestDeleteSheetFile { filename: meta_filename_to_delete });
                } else {
                    warn!("Cannot request file deletion for '{}': Metadata was missing.", sheet_name);
                }
            }
            Err(e) => {
                // This error means deletion from registry map failed, which is unexpected if sheet existed.
                let msg = format!("Critical error: Failed to delete sheet '{}' from registry: {}", sheet_name, e);
                error!("{}", msg);
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            }
        }
    }
}