// src/sheets/systems/logic/add_row.rs
use crate::sheets::{
    definitions::SheetMetadata, 
    events::{AddSheetRowRequest, SheetOperationFeedback, SheetDataModifiedInRegistryEvent}, 
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use std::collections::HashSet; // Keep if used elsewhere, not directly for add_row change

/// Handles adding a new, empty row to a specified sheet in a category.
/// MODIFIED: New rows are now inserted at the top (index 0).
pub fn handle_add_row_request(
    mut events: EventReader<AddSheetRowRequest>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    for event in events.read() {
        let category = &event.category; 
        let sheet_name = &event.sheet_name;

        let mut metadata_cache: Option<SheetMetadata> = None;

        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_cols = metadata.columns.len();
                // MODIFIED: Insert new row at the beginning (index 0)
                sheet_data.grid.insert(0, vec![String::new(); num_cols]);
                
                let msg =
                    format!("Added new row at the top of sheet '{:?}/{}'.", category, sheet_name);
                info!("{}", msg);
                feedback_writer.send(SheetOperationFeedback {
                    message: msg,
                    is_error: false,
                });

                metadata_cache = Some(metadata.clone());

                data_modified_writer.send(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });

            } else {
                let msg = format!(
                    "Cannot add row to sheet '{:?}/{}': Metadata missing.",
                    category, sheet_name
                );
                warn!("{}", msg);
                feedback_writer.send(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        } else {
            let msg = format!(
                "Cannot add row: Sheet '{:?}/{}' not found in registry.",
                category, sheet_name
            );
            warn!("{}", msg);
            feedback_writer.send(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
        }

        if let Some(meta_to_save) = metadata_cache {
            info!(
                "Row added to '{:?}/{}', triggering immediate save.",
                category, sheet_name
            );
            let registry_immut = registry.as_ref();
            save_single_sheet(registry_immut, &meta_to_save); 
        }
    }
}