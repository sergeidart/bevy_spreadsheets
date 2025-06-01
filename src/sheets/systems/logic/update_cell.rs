// src/sheets/systems/logic/update_cell.rs
use bevy::prelude::*;
use std::collections::HashMap;
use crate::sheets::{
    definitions::SheetMetadata,
    // ADDED RequestSheetRevalidation
    events::{UpdateCellEvent, SheetOperationFeedback, SheetDataModifiedInRegistryEvent, RequestSheetRevalidation},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};

pub fn handle_cell_update(
    mut events: EventReader<UpdateCellEvent>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    // ADDED revalidation writer
    mut revalidate_writer: EventWriter<RequestSheetRevalidation>,
) {
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();
    // ADDED: Track sheets needing revalidation
    let mut sheets_to_revalidate: HashMap<(Option<String>, String), ()> = HashMap::new();


    for event in events.read() {
        let category = &event.category;
        let sheet_name = &event.sheet_name;
        let row_idx = event.row_index;
        let col_idx = event.col_index;
        let new_value = &event.new_value;

        let validation_result: Result<(), String> = {
            let registry_immut = registry.as_ref();
            if let Some(sheet_data) = registry_immut.get_sheet(category, sheet_name) {
                if let Some(row) = sheet_data.grid.get(row_idx) {
                     if row.get(col_idx).is_some() { Ok(()) }
                     else { Err(format!("Column index {} out of bounds ({} columns).", col_idx, row.len())) }
                } else { Err(format!("Row index {} out of bounds ({} rows).", row_idx, sheet_data.grid.len())) }
            } else { Err(format!("Sheet '{:?}/{}' not found.", category, sheet_name)) }
        };

        match validation_result {
            Ok(()) => {
                if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
                    if let Some(row) = sheet_data.grid.get_mut(row_idx) {
                        if let Some(cell) = row.get_mut(col_idx) {
                            if *cell != *new_value {
                                trace!("Updating cell [{},{}] in sheet '{:?}/{}' from '{}' to '{}'", row_idx, col_idx, category, sheet_name, cell, new_value);
                                *cell = new_value.clone();

                                if let Some(metadata) = &sheet_data.metadata {
                                    let key = (category.clone(), sheet_name.clone());
                                    sheets_to_save.insert(key.clone(), metadata.clone());
                                    // Mark for revalidation
                                    sheets_to_revalidate.insert(key, ());

                                    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                                        category: category.clone(),
                                        sheet_name: sheet_name.clone(),
                                    });
                                } else {
                                     warn!("Cannot mark sheet '{:?}/{}' for save/revalidation after cell update: Metadata missing.", category, sheet_name);
                                }
                            } else {
                                trace!("Cell value unchanged for '{:?}/{}' cell[{},{}]. Skipping update.", category, sheet_name, row_idx, col_idx);
                            }
                        } else { error!("Cell update failed for '{:?}/{}' cell[{},{}]: Column index invalid.", category, sheet_name, row_idx, col_idx); }
                    } else { error!("Cell update failed for '{:?}/{}' cell[{},{}]: Row index invalid.", category, sheet_name, row_idx, col_idx); }
                } else { error!("Cell update failed for '{:?}/{}' cell[{},{}]: Sheet invalid.", category, sheet_name, row_idx, col_idx); }
            }
            Err(err_msg) => {
                 let full_msg = format!("Cell update rejected for sheet '{:?}/{}' cell[{},{}]: {}", category, sheet_name, row_idx, col_idx, err_msg);
                 warn!("{}", full_msg);
                 feedback_writer.write(SheetOperationFeedback { message: full_msg, is_error: true });
            }
        }
    }

    // Save sheets that were modified
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!("Cell updated in '{:?}/{}', triggering immediate save.", cat, name);
            save_single_sheet(registry_immut, &metadata);
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