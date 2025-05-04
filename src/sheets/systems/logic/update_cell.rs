// src/sheets/systems/logic/update_cell.rs
use bevy::prelude::*;
use crate::sheets::{
    definitions::{ColumnValidator}, // Keep ColumnValidator for potential future validation logic
    events::{UpdateCellEvent, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};

/// Handles updates to individual cell values within a sheet's grid data.
pub fn handle_cell_update(
    mut events: EventReader<UpdateCellEvent>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    let mut sheets_to_save = Vec::new(); // Track sheets needing save

    for event in events.read() {
        let sheet_name = &event.sheet_name;
        let row_idx = event.row_index;
        let col_idx = event.col_index;
        let new_value = &event.new_value;

        // --- Phase 1: Get current value and check bounds (Immutable Borrow) ---
        let validation_result: Result<String, String> = { // Returns Ok(old_value) or Err(msg)
            let registry_immut = registry.as_ref();
            if let Some(sheet_data) = registry_immut.get_sheet(sheet_name) {
                if let Some(row) = sheet_data.grid.get(row_idx) {
                     if let Some(cell) = row.get(col_idx) {
                         // TODO: Add validation logic here if needed in the future.
                         // Currently, validation happens visually in the UI,
                         // and backend accepts the string as provided by UI event.
                         // Example:
                         // if let Some(metadata) = &sheet_data.metadata {
                         //     if let Some(Some(validator)) = metadata.column_validators.get(col_idx) {
                         //         match validator {
                         //             ColumnValidator::Basic(_) => { /* validate basic type if needed */ }
                         //             ColumnValidator::Linked { .. } => { /* validate against allowed values if needed */ }
                         //         }
                         //     }
                         // }
                         Ok(cell.clone()) // Return current value if valid so far
                     } else {
                         Err(format!("Column index {} out of bounds ({} columns).", col_idx, row.len()))
                     }
                } else {
                     Err(format!("Row index {} out of bounds ({} rows).", row_idx, sheet_data.grid.len()))
                }
            } else {
                Err("Sheet not found.".to_string())
            }
        };

        // --- Phase 2: Application (Mutable Borrow) ---
        match validation_result {
            Ok(old_value) => {
                // Only update if the value actually changed
                if old_value != *new_value {
                     // Get mutable access to the sheet
                     if let Some(sheet_data) = registry.get_sheet_mut(sheet_name) {
                         // Bounds should be correct based on Phase 1 check, but use get_mut for safety
                         if let Some(row) = sheet_data.grid.get_mut(row_idx) {
                             if let Some(cell) = row.get_mut(col_idx) {
                                 trace!("Updating cell [{},{}] in sheet '{}' from '{}' to '{}'", row_idx, col_idx, sheet_name, old_value, new_value);
                                 *cell = new_value.clone(); // Update the cell value
                                 // Mark sheet for saving if not already marked
                                 if !sheets_to_save.contains(sheet_name) {
                                     sheets_to_save.push(sheet_name.clone());
                                 }
                             }
                             // Log error if cell index is somehow invalid after row check
                             else { error!("Cell update failed for '{}' cell[{},{}]: Column index became invalid.", sheet_name, row_idx, col_idx); }
                         }
                         // Log error if row index is somehow invalid after sheet check
                         else { error!("Cell update failed for '{}' cell[{},{}]: Row index became invalid.", sheet_name, row_idx, col_idx); }
                     }
                     // Log error if sheet is somehow missing after initial check
                     else { error!("Cell update failed for '{}' cell[{},{}]: Sheet became unavailable.", sheet_name, row_idx, col_idx); }
                 } else {
                      trace!("Cell value unchanged for '{}' cell[{},{}]. Skipping update.", sheet_name, row_idx, col_idx);
                 }
            }
            Err(err_msg) => {
                 // Log validation errors from Phase 1
                 let full_msg = format!("Cell update rejected for sheet '{}' cell[{},{}]: {}", sheet_name, row_idx, col_idx, err_msg);
                 warn!("{}", full_msg);
                 feedback_writer.send(SheetOperationFeedback { message: full_msg, is_error: true });
            }
        }
    } // End event loop

    // --- Phase 3: Saving ---
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref(); // Get immutable borrow for saving
        for sheet_name in sheets_to_save {
            info!("Cell updated in '{}', triggering immediate save.", sheet_name);
            save_single_sheet(registry_immut, &sheet_name);
        }
    }
}