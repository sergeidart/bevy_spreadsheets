// src/sheets/systems/logic/update_column_width.rs
use crate::sheets::{
    definitions::SheetMetadata, // Need metadata for saving
    events::{RequestUpdateColumnWidth, SheetOperationFeedback}, // Use the new event
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use std::collections::HashMap;

/// Handles requests to update the width of a specific column in a sheet's metadata.
pub fn handle_update_column_width(
    mut events: EventReader<RequestUpdateColumnWidth>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    let mut changed_sheets: HashMap<(Option<String>, String), SheetMetadata> =
        HashMap::new();

    for event in events.read() {
        let category = &event.category;
        let sheet_name = &event.sheet_name;
        let col_index = event.column_index;
        let new_width = event.new_width;

        let mut success = false;
        let mut metadata_cache: Option<SheetMetadata> = None;

        // --- Validation ---
        // Add basic width validation if needed (e.g., ensure positive)
        if new_width <= 0.0 {
            feedback_writer.send(SheetOperationFeedback {
                message: format!(
                    "Failed column width update in '{:?}/{}': Width must be positive.",
                    category, sheet_name
                ),
                is_error: true,
            });
            continue; // Skip invalid width
        }

        // --- Apply Update ---
        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &mut sheet_data.metadata {
                if let Some(column_def) = metadata.columns.get_mut(col_index) {
                    // Only update if the width actually changed (handle f32 comparison carefully)
                    let current_width = column_def.width;
                    const EPSILON: f32 = 0.1; // Tolerance for float comparison
                    if current_width.is_none() || (current_width.unwrap() - new_width).abs() > EPSILON {
                        trace!(
                            "Updating column {} width in sheet '{:?}/{}' from {:?} to {:.1}",
                            col_index + 1,
                            category,
                            sheet_name,
                            current_width,
                            new_width
                        );
                        column_def.width = Some(new_width);
                        success = true;
                        metadata_cache = Some(metadata.clone()); // Cache metadata for saving
                    } else {
                         trace!(
                             "Column {} width unchanged for sheet '{:?}/{}' (approx {:.1}). Skipping.",
                             col_index + 1, category, sheet_name, new_width
                         );
                    }
                } else {
                    feedback_writer.send(SheetOperationFeedback {
                        message: format!(
                            "Failed column width update in '{:?}/{}': Index {} out of bounds ({} columns).",
                            category,
                            sheet_name,
                            col_index,
                            metadata.columns.len()
                        ),
                        is_error: true,
                    });
                }
            } else {
                feedback_writer.send(SheetOperationFeedback {
                    message: format!(
                        "Failed column width update in '{:?}/{}': Metadata missing.",
                        category, sheet_name
                    ),
                    is_error: true,
                });
            }
        } else {
            feedback_writer.send(SheetOperationFeedback {
                message: format!(
                    "Failed column width update: Sheet '{:?}/{}' not found.",
                    category, sheet_name
                ),
                is_error: true,
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

    // --- Trigger Saves ---
    if !changed_sheets.is_empty() {
        let registry_immut = registry.as_ref(); // Immutable borrow for saving
        for ((cat, name), metadata) in changed_sheets {
            info!(
                "Column width updated for '{:?}/{}', triggering save.",
                cat, name
            );
            save_single_sheet(registry_immut, &metadata); // Pass metadata
        }
    }
}