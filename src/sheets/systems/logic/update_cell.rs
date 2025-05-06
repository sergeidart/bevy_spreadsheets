// src/sheets/systems/logic/update_cell.rs
use bevy::prelude::*;
use std::collections::HashMap; // <<< ADDED import for HashMap
use crate::sheets::{
    definitions::{SheetMetadata, ColumnValidator}, // Need metadata for saving
    events::{UpdateCellEvent, SheetOperationFeedback, SheetDataModifiedInRegistryEvent}, // Added SheetDataModifiedInRegistryEvent
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};

/// Handles updates to individual cell values within a sheet's grid data.
pub fn handle_cell_update(
    mut events: EventReader<UpdateCellEvent>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>, // Added writer
) {
    // Use a map to track which sheets need saving to avoid redundant saves per frame
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category; 
        let sheet_name = &event.sheet_name;
        let row_idx = event.row_index;
        let col_idx = event.col_index;
        let new_value = &event.new_value;

        let validation_result: Result<String, String> = { 
            let registry_immut = registry.as_ref();
            if let Some(sheet_data) = registry_immut.get_sheet(category, sheet_name) {
                if let Some(row) = sheet_data.grid.get(row_idx) {
                     if let Some(cell) = row.get(col_idx) {
                         Ok(cell.clone()) 
                     } else {
                         Err(format!("Column index {} out of bounds ({} columns).", col_idx, row.len()))
                     }
                } else {
                     Err(format!("Row index {} out of bounds ({} rows).", row_idx, sheet_data.grid.len()))
                }
            } else {
                Err(format!("Sheet '{:?}/{}' not found.", category, sheet_name))
            }
        };

        match validation_result {
            Ok(old_value) => {
                if old_value != *new_value {
                     if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
                         if let Some(row) = sheet_data.grid.get_mut(row_idx) {
                             if let Some(cell) = row.get_mut(col_idx) {
                                 trace!("Updating cell [{},{}] in sheet '{:?}/{}' from '{}' to '{}'", row_idx, col_idx, category, sheet_name, old_value, new_value);
                                 *cell = new_value.clone(); 

                                 if let Some(metadata) = &sheet_data.metadata {
                                     let key = (category.clone(), sheet_name.clone());
                                     sheets_to_save.insert(key, metadata.clone());
                                     // Send data modified event
                                     data_modified_writer.send(SheetDataModifiedInRegistryEvent {
                                         category: category.clone(),
                                         sheet_name: sheet_name.clone(),
                                     });
                                 } else {
                                      warn!("Cannot mark sheet '{:?}/{}' for save after cell update: Metadata missing.", category, sheet_name);
                                 }

                             } else { error!("Cell update failed for '{:?}/{}' cell[{},{}]: Column index became invalid.", category, sheet_name, row_idx, col_idx); }
                         } else { error!("Cell update failed for '{:?}/{}' cell[{},{}]: Row index became invalid.", category, sheet_name, row_idx, col_idx); }
                     } else { error!("Cell update failed for '{:?}/{}' cell[{},{}]: Sheet became unavailable.", category, sheet_name, row_idx, col_idx); }
                 } else {
                      trace!("Cell value unchanged for '{:?}/{}' cell[{},{}]. Skipping update.", category, sheet_name, row_idx, col_idx);
                 }
            }
            Err(err_msg) => {
                 let full_msg = format!("Cell update rejected for sheet '{:?}/{}' cell[{},{}]: {}", category, sheet_name, row_idx, col_idx, err_msg);
                 warn!("{}", full_msg);
                 feedback_writer.send(SheetOperationFeedback { message: full_msg, is_error: true });
            }
        }
    } 

    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref(); 
        for ((cat, name), metadata) in sheets_to_save {
            info!("Cell updated in '{:?}/{}', triggering immediate save.", cat, name);
            save_single_sheet(registry_immut, &metadata); 
        }
    }
}