// src/sheets/systems/logic/update_column_name.rs
use bevy::prelude::*;
use std::collections::HashMap; // <<< ADDED import for HashMap
use crate::sheets::{
    definitions::SheetMetadata, // Need metadata for saving
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
    // Use map to track sheets needing save
    // HashMap should now be found >>>
    let mut changed_sheets: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category; // <<< Get category
        let sheet_name = &event.sheet_name;
        let col_index = event.column_index;
        let new_name = event.new_name.trim(); // Trim whitespace

        let mut success = false; // Track if update was successful for this event
        let mut metadata_cache: Option<SheetMetadata> = None; // Cache metadata for saving

        // --- Validation ---
        if new_name.is_empty() {
            feedback_writer.send(SheetOperationFeedback{
                message: format!("Failed column rename in '{:?}/{}': New name cannot be empty.", category, sheet_name),
                is_error: true
            });
            continue; // Skip to next event
        }

        // --- Apply Update (Mutable Borrow) ---
        // Get sheet using category
        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &mut sheet_data.metadata {
                if col_index < metadata.column_headers.len() {
                    // Check for duplicate names (case-insensitive) in other columns
                    if metadata.column_headers.iter().enumerate()
                        .any(|(idx, h)| idx != col_index && h.eq_ignore_ascii_case(new_name))
                    {
                         feedback_writer.send(SheetOperationFeedback{
                             message: format!("Failed column rename in '{:?}/{}': Name '{}' already exists.", category, sheet_name, new_name),
                             is_error: true
                         });
                         // continue; // Decide if duplicate check should stop processing
                    } else {
                        // Perform the rename
                        let old_name = std::mem::replace(&mut metadata.column_headers[col_index], new_name.to_string());
                        let msg = format!("Renamed column {} in sheet '{:?}/{}' from '{}' to '{}'.", col_index + 1, category, sheet_name, old_name, new_name);
                        info!("{}", msg);
                        feedback_writer.send(SheetOperationFeedback{ message: msg, is_error: false });
                        success = true;
                        metadata_cache = Some(metadata.clone()); // Cache metadata for saving
                    }
                } else {
                    feedback_writer.send(SheetOperationFeedback{
                        message: format!("Failed column rename in '{:?}/{}': Index {} out of bounds ({} columns).", category, sheet_name, col_index, metadata.column_headers.len()),
                        is_error: true
                    });
                }
            } else {
                 feedback_writer.send(SheetOperationFeedback{
                     message: format!("Failed column rename in '{:?}/{}': Metadata missing.", category, sheet_name),
                     is_error: true
                 });
            }
        } else {
            feedback_writer.send(SheetOperationFeedback{
                message: format!("Failed column rename: Sheet '{:?}/{}' not found.", category, sheet_name),
                is_error: true
            });
        }

        // If successful, mark sheet for saving
        if success {
            if let Some(meta) = metadata_cache {
                 let key = (category.clone(), sheet_name.clone());
                 changed_sheets.insert(key, meta);
            }
        }
    } // End event loop

    // --- Trigger Saves (Immutable Borrow) ---
    if !changed_sheets.is_empty() {
        let registry_immut = registry.as_ref(); // Get immutable borrow for saving
        for ((cat, name), metadata) in changed_sheets {
            info!("Column name updated for '{:?}/{}', triggering save.", cat, name);
            save_single_sheet(registry_immut, &metadata); // <<< Pass metadata
        }
    }
}