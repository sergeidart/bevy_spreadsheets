// src/sheets/systems/logic.rs
use bevy::prelude::*;
use super::super::{
    events::AddSheetRowRequest,
    resources::SheetRegistry,
};

/// Handles the `AddSheetRowRequest` event sent from the UI.
pub fn handle_add_row_request(
    mut events: EventReader<AddSheetRowRequest>,
    mut registry: ResMut<SheetRegistry>,
) {
    for event in events.read() {
        // Use &event.sheet_name which is a &String, compatible with &str lookup
        if let Some(sheet_data) = registry.get_sheet_mut(&event.sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                // Use owned metadata
                let num_cols = metadata.column_headers.len();
                if num_cols > 0 {
                    // Add a new row with empty strings
                    sheet_data.grid.push(vec![String::new(); num_cols]);
                    info!("Added row to sheet '{}'", event.sheet_name);
                } else {
                    warn!("Cannot add row to sheet '{}': No columns defined in metadata.", event.sheet_name);
                }
            } else {
                 warn!("Cannot add row to sheet '{}': Metadata missing.", event.sheet_name);
            }
        } else {
            warn!("Cannot add row: Sheet '{}' not found in registry.", event.sheet_name);
        }
    }
}