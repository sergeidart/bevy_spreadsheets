// src/sheets/systems/logic/delete_rows.rs
use crate::sheets::{
    definitions::SheetMetadata, // Need metadata for saving
    events::{RequestDeleteRows, SheetDataModifiedInRegistryEvent, SheetOperationFeedback}, // Added SheetDataModifiedInRegistryEvent
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
    editor_state: Option<Res<crate::ui::elements::editor::state::EditorWindowState>>, // To map virtual sheets to parent contexts
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
                category,
                sheet_name
            );
            continue;
        }

        // Check if this is a virtual structure sheet
        let mut is_virtual = false;
        let mut parent_ctx_opt = None;
        if let Some(state) = editor_state.as_ref() {
            if sheet_name.starts_with("__virtual__") {
                // Find corresponding context
                if let Some(vctx) = state
                    .virtual_structure_stack
                    .iter()
                    .find(|v| &v.virtual_sheet_name == sheet_name)
                {
                    is_virtual = true;
                    parent_ctx_opt = Some(vctx.parent.clone());
                }
            }
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
            error_message = Some(format!("Sheet '{:?}/{}' not found.", category, sheet_name));
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
                warn!(
                    "Rows deleted from '{:?}/{}' but cannot save: Metadata missing.",
                    category, sheet_name
                );
            }

            // If virtual sheet rows were deleted, propagate back to original parent cell JSON
            if is_virtual {
                if let Some(parent_ctx) = parent_ctx_opt.clone() {
                    // Reconstruct JSON from virtual sheet current grid (after deletion)
                    if let Some(vsheet) = registry.get_sheet(category, sheet_name) {
                        if vsheet.metadata.is_some() {
                            // Clone required virtual sheet info before mutable borrow of registry
                            let v_rows: Vec<Vec<String>> = vsheet.grid.clone();
                            let _ = vsheet; // release immutable borrow
                            if let Some(parent_sheet_data) = registry
                                .get_sheet_mut(&parent_ctx.parent_category, &parent_ctx.parent_sheet)
                            {
                                if parent_ctx.parent_row < parent_sheet_data.grid.len() {
                                    if let Some(parent_row) =
                                        parent_sheet_data.grid.get_mut(parent_ctx.parent_row)
                                    {
                                        if parent_ctx.parent_col < parent_row.len() {
                                            // Build array of objects/arrays (one per virtual sheet row)
                                            let new_json = if v_rows.is_empty() {
                                                // All rows deleted => empty array
                                                "[]".to_string()
                                            } else if v_rows.len() == 1 {
                                                // Single row => store as array of strings
                                                let row_vals = v_rows.get(0).cloned().unwrap_or_default();
                                                serde_json::Value::Array(
                                                    row_vals
                                                        .into_iter()
                                                        .map(serde_json::Value::String)
                                                        .collect(),
                                                )
                                                .to_string()
                                            } else {
                                                // Multi row => array of arrays
                                                let outer: Vec<serde_json::Value> = v_rows
                                                    .iter()
                                                    .map(|r| {
                                                        serde_json::Value::Array(
                                                            r.iter()
                                                                .cloned()
                                                                .map(serde_json::Value::String)
                                                                .collect(),
                                                        )
                                                    })
                                                    .collect();
                                                serde_json::Value::Array(outer).to_string()
                                            };
                                            if let Some(cell_ref) =
                                                parent_row.get_mut(parent_ctx.parent_col)
                                            {
                                                if *cell_ref != new_json {
                                                    *cell_ref = new_json.clone();
                                                    info!("Propagated structure row deletion to parent cell '{:?}/{}'[{},{}]", 
                                                          parent_ctx.parent_category, parent_ctx.parent_sheet, 
                                                          parent_ctx.parent_row, parent_ctx.parent_col);
                                                    if let Some(pmeta) = &parent_sheet_data.metadata {
                                                        let key = (
                                                            parent_ctx.parent_category.clone(),
                                                            parent_ctx.parent_sheet.clone(),
                                                        );
                                                        sheets_to_save.insert(key.clone(), pmeta.clone());
                                                        data_modified_writer.write(
                                                            SheetDataModifiedInRegistryEvent {
                                                                category: parent_ctx
                                                                    .parent_category
                                                                    .clone(),
                                                                sheet_name: parent_ctx
                                                                    .parent_sheet
                                                                    .clone(),
                                                            },
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else if let Some(err) = error_message {
            // Only send feedback if the whole operation failed (e.g., sheet not found)
            error!(
                "Failed to delete rows from '{:?}/{}': {}",
                category, sheet_name, err
            );
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
            info!("Rows deleted in '{:?}/{}', triggering save.", cat, name);
            save_single_sheet(registry_immut, &metadata); // Pass metadata
        }
    }
}
