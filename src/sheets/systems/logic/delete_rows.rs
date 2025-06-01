// src/sheets/systems/logic/delete_rows.rs
use crate::sheets::{
    definitions::SheetMetadata, // Need metadata for saving
    events::{RequestDeleteRows, SheetOperationFeedback, SheetDataModifiedInRegistryEvent}, // Added SheetDataModifiedInRegistryEvent
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use std::collections::HashMap;

/// Handles deleting one or more specified rows from a sheet.
pub fn handle_delete_rows_request(
    mut events: EventReader<RequestDeleteRows>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>, // Added writer
) {
    // Use map to track sheets needing save after deletions
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category;
        let sheet_name = &event.sheet_name;
        let indices_to_delete = &event.row_indices;

        if indices_to_delete.is_empty() {
            trace!(
                "Skipping delete request for '{:?}/{}': No indices provided.",
                category, sheet_name
            );
            continue;
        }

        // --- Perform Deletion (Mutable Borrow) ---
        let mut operation_successful = false;
        let mut deleted_count = 0;
        let mut error_message: Option<String> = None;
        let mut metadata_cache: Option<SheetMetadata> = None;

        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            // Sort indices descending to avoid index shifting issues during removal
            let mut sorted_indices: Vec<usize> = indices_to_delete.iter().cloned().collect();
            sorted_indices.sort_unstable_by(|a, b| b.cmp(a)); // Sort descending

            let initial_row_count = sheet_data.grid.len();

            for &index in &sorted_indices {
                if index < sheet_data.grid.len() {
                    sheet_data.grid.remove(index);
                    deleted_count += 1;
                } else {
                    error_message = Some(format!(
                        "Index {} out of bounds ({} rows). Deletion partially failed.",
                        index, initial_row_count
                    ));
                    // Stop processing further indices for this event on error? Or just skip?
                    // Let's just skip the invalid index and report the partial failure.
                    warn!(
                        "Skipping delete for index {} in '{:?}/{}': Out of bounds.",
                        index, category, sheet_name
                    );
                }
            }

            if deleted_count > 0 {
                operation_successful = true; // Mark successful even if partial
                // Cache metadata for saving if deletion occurred
                metadata_cache = sheet_data.metadata.clone();
                // Send data modified event
                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });
            }

        } else {
            error_message = Some(format!(
                "Sheet '{:?}/{}' not found.",
                category, sheet_name
            ));
        }

        // --- Feedback and Saving ---
        if operation_successful {
            let base_msg = format!(
                "Deleted {} row(s) from sheet '{:?}/{}'.",
                deleted_count, category, sheet_name
            );
            let final_msg = if let Some(ref err) = error_message {
                format!("{} {}", base_msg, err) // Append error if partial failure
            } else {
                base_msg
            };
            info!("{}", final_msg); // Log full message
            feedback_writer.write(SheetOperationFeedback {
                message: final_msg,
                is_error: error_message.is_some(), // Mark as error only if partial failure occurred
            });

            // Add to save list if metadata was found
            if let Some(meta) = metadata_cache {
                let key = (category.clone(), sheet_name.clone());
                sheets_to_save.insert(key, meta);
            } else if deleted_count > 0 {
                 warn!("Rows deleted from '{:?}/{}' but cannot save: Metadata missing.", category, sheet_name);
            }
        } else if let Some(err) = error_message {
            // Only send feedback if the whole operation failed (e.g., sheet not found)
            error!("Failed to delete rows from '{:?}/{}': {}", category, sheet_name, err);
            feedback_writer.write(SheetOperationFeedback {
                message: format!("Delete failed for '{:?}/{}': {}", category, sheet_name, err),
                is_error: true,
            });
        }
    } // End event loop

    // --- Trigger Saves (Immutable Borrow) ---
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref(); // Get immutable borrow for saving
        for ((cat, name), metadata) in sheets_to_save {
            info!(
                "Rows deleted in '{:?}/{}', triggering save.",
                cat, name
            );
            save_single_sheet(registry_immut, &metadata); // Pass metadata
        }
    }
}