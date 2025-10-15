// src/sheets/systems/logic/update_cell/mod.rs
//! Cell update system - handles user-initiated cell value changes

mod cascade;
mod cell_update;
mod db_persistence;
mod validation;
mod virtual_sheet;

use crate::sheets::{
    definitions::SheetMetadata,
    events::{
        RequestSheetRevalidation, SheetDataModifiedInRegistryEvent, SheetOperationFeedback,
        UpdateCellEvent,
    },
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use std::collections::HashMap;

/// Main system handler for cell update events
pub fn handle_cell_update(
    mut events: EventReader<UpdateCellEvent>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut revalidate_writer: EventWriter<RequestSheetRevalidation>,
    editor_state: Option<Res<EditorWindowState>>,
) {
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();
    let mut sheets_to_revalidate: HashMap<(Option<String>, String), ()> = HashMap::new();

    for event in events.read() {
        let category = event.category.clone();
        let sheet_name = event.sheet_name.clone();
        let row_idx = event.row_index;
        let col_idx = event.col_index;
        let new_value = &event.new_value;

        // Check if this is a virtual sheet
        let parent_ctx_opt = virtual_sheet::get_virtual_sheet_context(&sheet_name, &editor_state);
        let is_virtual = parent_ctx_opt.is_some();

        // Validate cell location
        let validation_result = validation::validate_cell_location(
            registry.as_ref(),
            &category,
            &sheet_name,
            row_idx,
            col_idx,
        );

        match validation_result {
            Ok(()) => {
                if let Some(sheet_data) = registry.get_sheet_mut(&category, &sheet_name) {
                    if let Some(row) = sheet_data.grid.get_mut(row_idx) {
                        // Extract column metadata
                        let col_meta = cell_update::extract_column_metadata(
                            &sheet_data.metadata,
                            col_idx,
                        );

                        // Update the cell value
                        let update_result = if let Some(cell) = row.get_mut(col_idx) {
                            cell_update::update_cell_value(
                                cell,
                                new_value,
                                &sheet_data.metadata,
                                col_idx,
                                row_idx,
                                &category,
                                &sheet_name,
                            )
                        } else {
                            error!(
                                "Cell update failed for '{:?}/{}' cell[{},{}]: Column index invalid.",
                                category, sheet_name, row_idx, col_idx
                            );
                            continue;
                        };

                        if update_result.changed {
                            if let Some(metadata) = &sheet_data.metadata {
                                let key = (category.clone(), sheet_name.clone());
                                sheets_to_save.insert(key.clone(), metadata.clone());
                                sheets_to_revalidate.insert(key, ());

                                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                                    category: category.clone(),
                                    sheet_name: sheet_name.clone(),
                                });

                                // Persist to database
                                if let Some(final_val) = &update_result.final_value {
                                    let _ = db_persistence::persist_cell_to_database(
                                        metadata,
                                        &sheet_name,
                                        &category,
                                        row,
                                        row_idx,
                                        col_idx,
                                        &col_meta.header,
                                        final_val,
                                        update_result.old_value.as_deref(),
                                        col_meta.is_structure_col,
                                        col_meta.looks_like_real_structure,
                                    );
                                }
                            } else {
                                warn!(
                                    "Cannot mark sheet '{:?}/{}' for save/revalidation after cell update: Metadata missing.",
                                    category, sheet_name
                                );
                            }
                        }
                    } else {
                        error!(
                            "Cell update failed for '{:?}/{}' cell[{},{}]: Row index invalid.",
                            category, sheet_name, row_idx, col_idx
                        );
                    }
                } else {
                    error!(
                        "Cell update failed for '{:?}/{}' cell[{},{}]: Sheet invalid.",
                        category, sheet_name, row_idx, col_idx
                    );
                }
            }
            Err(err_msg) => {
                let full_msg = format!(
                    "Cell update rejected for sheet '{:?}/{}' cell[{},{}]: {}",
                    category, sheet_name, row_idx, col_idx, err_msg
                );
                warn!("{}", full_msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: full_msg,
                    is_error: true,
                });
            }
        }

        // Sync virtual sheet changes back to parent
        if is_virtual {
            if let Some(parent_ctx) = parent_ctx_opt {
                let _ = virtual_sheet::sync_virtual_sheet_to_parent(
                    registry.as_mut(),
                    &category,
                    &sheet_name,
                    &parent_ctx,
                    &mut sheets_to_save,
                    &mut sheets_to_revalidate,
                    &mut data_modified_writer,
                );
            }
        }
    }

    // Save sheets that were modified
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!(
                "Cell updated in '{:?}/{}', triggering immediate save.",
                cat, name
            );
            if metadata.category.is_none() {
                save_single_sheet(registry_immut, &metadata);
            }
        }
    }

    // Send revalidation requests
    for (cat, name) in sheets_to_revalidate.keys() {
        revalidate_writer.write(RequestSheetRevalidation {
            category: cat.clone(),
            sheet_name: name.clone(),
        });
        trace!("Sent revalidation request for sheet '{:?}/{}'.", cat, name);
    }
}
