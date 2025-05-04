// src/sheets/systems/logic/update_column_name.rs
use bevy::prelude::*;
use crate::sheets::{
    events::{RequestUpdateColumnName, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};

/// Handles requests to update the name of a specific column in a sheet's metadata.
pub fn handle_update_column_name(
    mut events: EventReader<RequestUpdateColumnName>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    let mut changed_sheets = Vec::new(); // Track sheets needing save

    for event in events.read() {
        let sheet_name = &event.sheet_name;
        let col_index = event.column_index;
        let new_name = event.new_name.trim(); // Trim whitespace

        let mut success = false; // Track if update was successful for this event

        // --- Validation ---
        if new_name.is_empty() {
            feedback_writer.send(SheetOperationFeedback{
                message: format!("Failed column rename in '{}': New name cannot be empty.", sheet_name),
                is_error: true
            });
            continue; // Skip to next event
        }

        // --- Apply Update (Mutable Borrow) ---
        if let Some(sheet_data) = registry.get_sheet_mut(sheet_name) {
            if let Some(metadata) = &mut sheet_data.metadata {
                if col_index < metadata.column_headers.len() {
                    // Check for duplicate names (case-insensitive) in other columns
                    if metadata.column_headers.iter().enumerate()
                        .any(|(idx, h)| idx != col_index && h.eq_ignore_ascii_case(new_name))
                    {
                         feedback_writer.send(SheetOperationFeedback{
                             message: format!("Failed column rename in '{}': Name '{}' already exists.", sheet_name, new_name),
                             is_error: true
                         });
                         // continue; // Decide if duplicate check should stop processing
                    } else {
                        // Perform the rename
                        let old_name = std::mem::replace(&mut metadata.column_headers[col_index], new_name.to_string());
                        let msg = format!("Renamed column {} in sheet '{}' from '{}' to '{}'.", col_index + 1, sheet_name, old_name, new_name);
                        info!("{}", msg);
                        feedback_writer.send(SheetOperationFeedback{ message: msg, is_error: false });
                        success = true;
                    }
                } else {
                    feedback_writer.send(SheetOperationFeedback{
                        message: format!("Failed column rename in '{}': Index {} out of bounds ({} columns).", sheet_name, col_index, metadata.column_headers.len()),
                        is_error: true
                    });
                }
            } else {
                 feedback_writer.send(SheetOperationFeedback{
                     message: format!("Failed column rename in '{}': Metadata missing.", sheet_name),
                     is_error: true
                 });
            }
        } else {
            feedback_writer.send(SheetOperationFeedback{
                message: format!("Failed column rename: Sheet '{}' not found.", sheet_name),
                is_error: true
            });
        }

        // If successful, mark sheet for saving
        if success && !changed_sheets.contains(sheet_name) {
            changed_sheets.push(sheet_name.clone());
        }
    } // End event loop

    // --- Trigger Saves (Immutable Borrow) ---
    if !changed_sheets.is_empty() {
        let registry_immut = registry.as_ref(); // Get immutable borrow for saving
        for sheet_name in changed_sheets {
            info!("Column name updated for '{}', triggering save.", sheet_name);
            save_single_sheet(registry_immut, &sheet_name);
        }
    }
}