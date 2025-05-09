// src/sheets/systems/logic/delete_columns.rs
use crate::sheets::{
    definitions::SheetMetadata,
    events::{RequestDeleteColumns, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

pub fn handle_delete_columns_request(
    mut events: EventReader<RequestDeleteColumns>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category;
        let sheet_name = &event.sheet_name;
        let indices_to_delete = &event.column_indices;

        if indices_to_delete.is_empty() {
            trace!(
                "Skipping delete columns request for '{:?}/{}': No indices provided.",
                category,
                sheet_name
            );
            continue;
        }

        let mut operation_successful = false;
        let mut deleted_count = 0;
        let mut error_message: Option<String> = None;
        let mut metadata_cache: Option<SheetMetadata> = None;

        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &mut sheet_data.metadata {
                // Sort indices descending to avoid shifting issues during removal
                let mut sorted_indices: Vec<usize> = indices_to_delete.iter().cloned().collect();
                sorted_indices.sort_unstable_by(|a, b| b.cmp(a)); // Sort descending

                let initial_col_count = metadata.columns.len();

                for &col_idx_to_remove in &sorted_indices {
                    if col_idx_to_remove < metadata.columns.len() {
                        // Remove from metadata
                        metadata.columns.remove(col_idx_to_remove);
                        // metadata.column_headers.remove(col_idx_to_remove);
                        // metadata.column_types.remove(col_idx_to_remove);
                        // metadata.column_validators.remove(col_idx_to_remove);
                        // metadata.column_filters.remove(col_idx_to_remove);
                        // metadata.column_ai_contexts.remove(col_idx_to_remove);
                        // metadata.column_widths.remove(col_idx_to_remove);


                        // Remove from grid data
                        for row in sheet_data.grid.iter_mut() {
                            if col_idx_to_remove < row.len() {
                                row.remove(col_idx_to_remove);
                            }
                        }
                        deleted_count += 1;
                    } else {
                        let err_msg = format!(
                            "Column index {} out of bounds ({} columns). Deletion partially failed.",
                            col_idx_to_remove, initial_col_count
                        );
                        error_message = Some(err_msg.clone());
                        warn!(
                            "Skipping delete for column index {} in '{:?}/{}': Out of bounds. {}",
                            col_idx_to_remove, category, sheet_name, err_msg
                        );
                    }
                }

                if deleted_count > 0 {
                    metadata.ensure_column_consistency(); // Recalculate consistency if needed
                    operation_successful = true;
                    metadata_cache = Some(metadata.clone());
                    data_modified_writer.send(SheetDataModifiedInRegistryEvent {
                        category: category.clone(),
                        sheet_name: sheet_name.clone(),
                    });
                }
            } else {
                error_message = Some(format!(
                    "Metadata missing for sheet '{:?}/{}'. Cannot delete columns.",
                    category, sheet_name
                ));
            }
        } else {
            error_message = Some(format!(
                "Sheet '{:?}/{}' not found. Cannot delete columns.",
                category, sheet_name
            ));
        }

        if operation_successful {
            let base_msg = format!(
                "Deleted {} column(s) from sheet '{:?}/{}'.",
                deleted_count, category, sheet_name
            );
            let final_msg = if let Some(ref err) = error_message {
                format!("{} {}", base_msg, err)
            } else {
                base_msg
            };
            info!("{}", final_msg);
            feedback_writer.send(SheetOperationFeedback {
                message: final_msg,
                is_error: error_message.is_some(),
            });

            if let Some(meta) = metadata_cache {
                sheets_to_save.insert((category.clone(), sheet_name.clone()), meta);
            } else if deleted_count > 0 {
                warn!(
                    "Columns deleted from '{:?}/{}' but cannot save: Metadata missing or no columns actually deleted from metadata.",
                    category, sheet_name
                );
            }
        } else if let Some(err) = error_message {
            error!(
                "Failed to delete columns from '{:?}/{}': {}",
                category, sheet_name, err
            );
            feedback_writer.send(SheetOperationFeedback {
                message: format!(
                    "Column delete failed for '{:?}/{}': {}",
                    category, sheet_name, err
                ),
                is_error: true,
            });
        }
    }

    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!(
                "Columns deleted in '{:?}/{}', triggering save.",
                cat, name
            );
            save_single_sheet(registry_immut, &metadata);
        }
    }
}