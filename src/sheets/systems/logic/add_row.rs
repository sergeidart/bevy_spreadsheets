// src/sheets/systems/logic/add_row.rs
use bevy::prelude::*;
use crate::sheets::{
    definitions::SheetMetadata, // Need metadata to get column count
    events::{AddSheetRowRequest, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};

/// Handles adding a new, empty row to a specified sheet in a category.
pub fn handle_add_row_request(
    mut events: EventReader<AddSheetRowRequest>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    for event in events.read() {
        let category = &event.category; // <<< Get category
        let sheet_name = &event.sheet_name;

        let mut metadata_cache: Option<SheetMetadata> = None; // Cache metadata for saving

        // Get mutable sheet data
        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_cols = metadata.column_headers.len();
                // Add a new row with empty strings matching the number of columns
                sheet_data.grid.push(vec![String::new(); num_cols]);
                let msg = format!("Added row to sheet '{:?}/{}'.", category, sheet_name);
                info!("{}", msg);
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: false });

                // Cache metadata for saving
                metadata_cache = Some(metadata.clone());
            } else {
                 let msg = format!("Cannot add row to sheet '{:?}/{}': Metadata missing.", category, sheet_name);
                 warn!("{}", msg);
                 feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            }
        } else {
            let msg = format!("Cannot add row: Sheet '{:?}/{}' not found in registry.", category, sheet_name);
            warn!("{}", msg);
            feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
        }

        // Trigger save if a row was added and metadata was found
        if let Some(meta_to_save) = metadata_cache {
             info!("Row added to '{:?}/{}', triggering immediate save.", category, sheet_name);
             // Need immutable borrow for save
             let registry_immut = registry.as_ref();
             save_single_sheet(registry_immut, &meta_to_save); // <<< Pass metadata
        }
    }
}