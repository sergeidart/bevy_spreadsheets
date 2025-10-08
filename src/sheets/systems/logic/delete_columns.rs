// src/sheets/systems/logic/delete_columns.rs
use crate::sheets::{
    definitions::{SheetMetadata, ColumnValidator},
    events::{RequestDeleteColumns, RequestDeleteSheetFile, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

pub fn handle_delete_columns_request(
    mut events: EventReader<RequestDeleteColumns>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut file_delete_writer: EventWriter<RequestDeleteSheetFile>,
    editor_state: Option<Res<EditorWindowState>>,
) {
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();
    let mut structure_sheets_to_delete: Vec<(Option<String>, String)> = Vec::new();

    for event in events.read() {
        let (category, sheet_name) = if let Some(state) = editor_state.as_ref() {
            if let Some(vctx) = state.virtual_structure_stack.last() {
                (&event.category, &vctx.virtual_sheet_name)
            } else {
                (&event.category, &event.sheet_name)
            }
        } else {
            (&event.category, &event.sheet_name)
        };
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
                        // Check if this column has Structure validator - collect for cascade delete
                        if let Some(col_def) = metadata.columns.get(col_idx_to_remove) {
                            if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                                let structure_sheet_name = format!("{}_{}", sheet_name, col_def.header);
                                structure_sheets_to_delete.push((category.clone(), structure_sheet_name));
                            }
                        }
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
                    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
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
            feedback_writer.write(SheetOperationFeedback {
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
            feedback_writer.write(SheetOperationFeedback {
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
            info!("Columns deleted in '{:?}/{}', triggering save.", cat, name);
            if metadata.category.is_none() {
                save_single_sheet(registry_immut, &metadata);
            }
        }
    }

    // Cascade delete structure sheets
    if !structure_sheets_to_delete.is_empty() {
        for (struct_category, struct_sheet_name) in structure_sheets_to_delete {
            // Remove from registry
            if let Ok(_removed) = registry.delete_sheet(&struct_category, &struct_sheet_name) {
                info!(
                    "Removed structure sheet '{:?}/{}' from registry due to cascade delete.",
                    struct_category, struct_sheet_name
                );

                // Delete physical files (.json and .meta.json)
                let category_path = if let Some(ref cat) = struct_category {
                    format!("{}/", cat)
                } else {
                    String::new()
                };
                
                let json_path = PathBuf::from(format!("{}{}.json", category_path, struct_sheet_name));
                let meta_path = PathBuf::from(format!("{}{}.meta.json", category_path, struct_sheet_name));

                file_delete_writer.write(RequestDeleteSheetFile {
                    relative_path: json_path,
                });
                file_delete_writer.write(RequestDeleteSheetFile {
                    relative_path: meta_path,
                });

                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "Cascade deleted structure sheet '{:?}/{}'.",
                        struct_category, struct_sheet_name
                    ),
                    is_error: false,
                });
            } else {
                warn!(
                    "Attempted to cascade delete structure sheet '{:?}/{}' but it was not found in registry.",
                    struct_category, struct_sheet_name
                );
            }
        }
    }
}
