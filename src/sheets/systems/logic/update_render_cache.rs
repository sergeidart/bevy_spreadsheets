// src/sheets/systems/logic/update_render_cache.rs
use bevy::prelude::*;
use std::collections::HashSet; // For collecting unique sheet identifiers

use crate::{
    sheets::{
        definitions::{ColumnDataType, ColumnValidator},
        events::{
            // Potentially listen to other events that change sheet structure or metadata
            RequestDeleteSheet,
            RequestRenameSheet,
            RequestSheetRevalidation,
            SheetDataModifiedInRegistryEvent,
        },
        resources::{SheetRegistry, SheetRenderCache},
    },
    ui::{
        common::generate_structure_preview,
        elements::editor::state::EditorWindowState, // Needed for linked cache access during validation
        validation::{validate_basic_cell, validate_linked_cell, ValidationState},
    },
};

/// System that listens for various events and updates the SheetRenderCache.
/// This effectively replaces the old handle_sheet_revalidation_request system.
#[allow(clippy::too_many_arguments)]
pub fn handle_sheet_render_cache_update(
    // Event Readers:
    mut ev_revalidate: EventReader<RequestSheetRevalidation>,
    mut ev_data_modified: EventReader<SheetDataModifiedInRegistryEvent>,
    mut ev_sheet_deleted: EventReader<RequestDeleteSheet>, // To clear cache
    mut ev_sheet_renamed: EventReader<RequestRenameSheet>, // To rename cache entry
    // Consider adding readers for AddSheetRowRequest, RequestDeleteRows,
    // RequestUpdateColumnName, RequestUpdateColumnValidator if they
    // don't already fire SheetDataModifiedInRegistryEvent or RequestSheetRevalidation appropriately.
    // For now, we assume those events lead to one of the above.

    // Resources:
    registry: Res<SheetRegistry>,
    mut render_cache: ResMut<SheetRenderCache>,
    // Local state for editor, used by validate_linked_cell for its *own* cache.
    // This is a bit indirect, ideally validation wouldn't need mutable UI state.
    mut editor_state: Local<EditorWindowState>,
    // To react to selected sheet changes if needed (for proactive caching)
    // current_editor_state: Option<Res<EditorWindowState>>, // If needed from global
) {
    let mut sheets_to_rebuild: HashSet<(Option<String>, String)> = HashSet::new();

    // 1. Collect sheets explicitly requested for revalidation
    for event in ev_revalidate.read() {
        sheets_to_rebuild.insert((event.category.clone(), event.sheet_name.clone()));
    }

    // 2. Collect sheets where data was modified
    for event in ev_data_modified.read() {
        sheets_to_rebuild.insert((event.category.clone(), event.sheet_name.clone()));
    }

    // Handle cache clearing for deleted sheets
    for event in ev_sheet_deleted.read() {
        render_cache.clear_sheet_render_data(&event.category, &event.sheet_name);
        // Also remove from sheets_to_rebuild if it was marked, as it no longer exists
        sheets_to_rebuild.remove(&(event.category.clone(), event.sheet_name.clone()));
        debug!(
            "Cleared render cache for deleted sheet: '{:?}/{}'",
            event.category, event.sheet_name
        );
    }

    // Handle cache renaming for renamed sheets
    for event in ev_sheet_renamed.read() {
        render_cache.rename_sheet_render_data(&event.category, &event.old_name, &event.new_name);
        // If the old name was in sheets_to_rebuild, remove it and add the new one.
        if sheets_to_rebuild.remove(&(event.category.clone(), event.old_name.clone())) {
            sheets_to_rebuild.insert((event.category.clone(), event.new_name.clone()));
        }
        debug!(
            "Renamed render cache entry: '{:?}/{}' -> '{:?}/{}'",
            event.category, event.old_name, event.category, event.new_name
        );
    }

    if sheets_to_rebuild.is_empty() {
        return;
    }

    debug!(
        "Rebuilding render cache for sheets: {:?}",
        sheets_to_rebuild
    );

    for (category, sheet_name) in sheets_to_rebuild {
        if let Some(sheet_data) = registry.get_sheet(&category, &sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_rows = sheet_data.grid.len();
                let num_cols = metadata.columns.len();

                // Get a mutable reference to the sheet's cache, ensuring dimensions
                let current_sheet_render_cache = render_cache.ensure_and_get_sheet_cache_mut(
                    &category,
                    &sheet_name,
                    num_rows,
                    num_cols,
                );

                for r_idx in 0..num_rows {
                    for c_idx in 0..num_cols {
                        let cell_value_str = sheet_data
                            .grid
                            .get(r_idx)
                            .and_then(|row| row.get(c_idx))
                            .map(|s| s.as_str())
                            .unwrap_or(""); // Default to empty string if out of bounds

                        let col_def_opt = metadata.columns.get(c_idx);
                        // let mut validation_state_for_cell = ValidationState::default();
                        // let mut display_text_for_cell = cell_value_str.to_string(); // Default

                        if let Some(col_def) = col_def_opt {
                            // Perform validation
                            // Remove unused val_state assignment
                            let (_val_state, _allowed_values_opt) = match &col_def.validator {
                                Some(ColumnValidator::Basic(data_type)) => {
                                    let (state, _parse_error) =
                                        validate_basic_cell(cell_value_str, *data_type);
                                    (state, None)
                                }
                                Some(ColumnValidator::Linked {
                                    target_sheet_name,
                                    target_column_index,
                                }) => {
                                    validate_linked_cell(
                                        cell_value_str,
                                        target_sheet_name,
                                        *target_column_index,
                                        &registry,
                                        &mut editor_state, // Pass Local state mutably for linked_column_cache access
                                    )
                                }
                                Some(ColumnValidator::Structure) => {
                                    // Treat structure cells as always valid (content is JSON string) for now
                                    (ValidationState::Valid, None)
                                }
                                None => {
                                    // Treat as basic string if no validator
                                    let (state, _parse_error) =
                                        validate_basic_cell(cell_value_str, ColumnDataType::String);
                                    (state, None)
                                }
                            };
                            // Directly use validation in the render_cell assignment below
                        } else {
                            error!("Render Cache: Metadata column index mismatch for sheet '{:?}/{}' at column {}", category, sheet_name, c_idx);
                            // Directly use ValidationState::Invalid below
                        }

                        // Update the specific cell in the render cache
                        if let Some(render_cell) = current_sheet_render_cache
                            .get_mut(r_idx)
                            .and_then(|row| row.get_mut(c_idx))
                        {
                            // Custom preview for structure cells: friendly label using parent key and column name
                            let preview_text = if let Some(col_def) = col_def_opt {
                                if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                                    // Try to form "<Key> <ColumnName>" if possible
                                    let mut label: Option<String> = None;
                                    if let Some(parent_row) = registry
                                        .get_sheet(&category, &sheet_name)
                                        .and_then(|sd| sd.grid.get(r_idx))
                                    {
                                        // Parent key is usually first column (index 0)
                                        let parent_key =
                                            parent_row.get(0).cloned().unwrap_or_default();
                                        let col_name = col_def.header.trim();
                                        if !parent_key.trim().is_empty() {
                                            label = Some(format!("{} {}", parent_key, col_name));
                                        } else {
                                            label = Some(col_name.to_string());
                                        }
                                    }

                                    // Fallback to parsed JSON preview
                                    label.unwrap_or_else(|| {
                                        generate_structure_preview(cell_value_str).0
                                    })
                                } else {
                                    cell_value_str.to_string()
                                }
                            } else {
                                cell_value_str.to_string()
                            };
                            render_cell.display_text = preview_text;
                            render_cell.validation_state = if let Some(col_def) = col_def_opt {
                                // Use val_state from above
                                let (val_state, _) = match &col_def.validator {
                                    Some(ColumnValidator::Basic(data_type)) => {
                                        let (state, _) =
                                            validate_basic_cell(cell_value_str, *data_type);
                                        (state, None)
                                    }
                                    Some(ColumnValidator::Linked {
                                        target_sheet_name,
                                        target_column_index,
                                    }) => validate_linked_cell(
                                        cell_value_str,
                                        target_sheet_name,
                                        *target_column_index,
                                        &registry,
                                        &mut editor_state,
                                    ),
                                    Some(ColumnValidator::Structure) => {
                                        (ValidationState::Valid, None)
                                    }
                                    None => {
                                        let (state, _) = validate_basic_cell(
                                            cell_value_str,
                                            ColumnDataType::String,
                                        );
                                        (state, None)
                                    }
                                };
                                val_state
                            } else {
                                ValidationState::Invalid
                            };
                        } else {
                            // This should not happen if ensure_and_get_sheet_cache_mut worked correctly
                            error!("Render Cache: Index out of bounds when updating cell [{},{}] for sheet '{:?}/{}'. Dimensions: {}x{}", r_idx, c_idx, category, sheet_name, num_rows, num_cols);
                        }
                    } // end col loop
                } // end row loop
                debug!("Render cache updated for '{:?}/{}'.", category, sheet_name);
            } else {
                warn!(
                    "Cannot update render cache for sheet '{:?}/{}': Metadata missing.",
                    category, sheet_name
                );
                render_cache.clear_sheet_render_data(&category, &sheet_name);
            }
        } else {
            // Sheet not found in registry, ensure its render cache is also cleared
            trace!(
                "Sheet '{:?}/{}' not found in registry. Clearing its render cache.",
                category,
                sheet_name
            );
            render_cache.clear_sheet_render_data(&category, &sheet_name);
        }
    }
}
