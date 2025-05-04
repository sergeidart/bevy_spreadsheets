// src/sheets/systems/logic.rs
use bevy::prelude::*;
use super::super::{
    events::AddSheetRowRequest, // Event type
    resources::SheetRegistry,   // Resource type
};

/// Handles the `AddSheetRowRequest` event sent from the UI.
pub fn handle_add_row_request(
    mut events: EventReader<AddSheetRowRequest>,
    mut registry: ResMut<SheetRegistry>, // Needs mutable access to add row
) {
    for event in events.read() {
        if let Some(sheet_data) = registry.get_sheet_mut(event.sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_cols = metadata.column_headers.len();
                if num_cols > 0 {
                    // Add a new row with the correct number of empty strings
                    sheet_data.grid.push(vec![String::new(); num_cols]);
                } else {
                }
            } else {
            }
        } else {
        }
    }
}