// src/ui/elements/editor/table_body.rs
use bevy::prelude::*;
use bevy_egui::egui;
use egui_extras::{TableBody, TableRow};

use crate::sheets::{
    definitions::ColumnValidator,
    resources::SheetRegistry,
    events::UpdateCellEvent,
};
// Import the refactored function
use crate::ui::common::edit_cell_widget; // <-- CHANGED from ui_for_cell
use super::state::EditorWindowState;


/// Filters rows based on the "contains" logic for active column filters.
/// Returns a Vec containing the original indices of rows that pass ALL filters.
fn get_filtered_row_indices(
    grid: &Vec<Vec<String>>,
    filters: &[Option<String>],
    _headers: &[String], // Kept for potential future use
) -> Vec<usize> {
    if filters.iter().all(Option::is_none) {
        // Optimization: If no filters, return all indices
        return (0..grid.len()).collect();
    }
    (0..grid.len())
        .filter(|&row_idx| {
            if let Some(row) = grid.get(row_idx) {
                // Check if this row passes ALL active filters
                filters.iter().enumerate().all(|(col_idx, filter_opt)| {
                    match filter_opt {
                        // If filter exists and is not empty, check if cell contains it (case-insensitive)
                        Some(filter_text) if !filter_text.is_empty() => {
                             row.get(col_idx)
                                .map_or(false, |cell_text| cell_text.to_lowercase().contains(&filter_text.to_lowercase()))
                        }
                        // If no filter or filter is empty, this column passes
                        _ => true,
                    }
                })
            } else { false } // Row index out of bounds
        })
        .collect()
}


/// Renders the body rows for the sheet table, handling filtering and cell editing via Events.
/// Returns false (change handling is now done via events).
pub fn sheet_table_body(
    body: TableBody,
    row_height: f32,
    sheet_name: &str,
    headers: &[String], // Needed for column count consistency check
    filters: &[Option<String>], // Passed to filtering logic
    registry: &SheetRegistry, // Changed to immutable reference &SheetRegistry
    mut cell_update_writer: EventWriter<UpdateCellEvent>,
    state: &mut EditorWindowState, // Mutable state for cache access
) -> bool { // Return value less meaningful now
    let mut body = body;

    // --- Pre-fetch data needed for rendering (immutable reads) ---
    let (grid_data, filtered_indices, num_cols, validators) = {
        if let Some(sheet_data) = registry.get_sheet(sheet_name) {
            if let Some(meta) = &sheet_data.metadata {
                 (
                    &sheet_data.grid, // Borrow grid directly
                    get_filtered_row_indices(&sheet_data.grid, filters, headers),
                    meta.column_headers.len(), // Use metadata headers length
                    meta.column_validators.clone(), // Clone validators for use in closure
                 )
            } else {
                 // Metadata missing, use empty defaults
                 warn!("Sheet '{}' found but metadata missing in table_body", sheet_name);
                 (&Vec::new(), Vec::new(), 0, Vec::new())
            }
        } else {
             // Sheet missing entirely
             warn!("Sheet '{}' not found in registry in table_body", sheet_name);
             (&Vec::new(), Vec::new(), 0, Vec::new())
        }
    };

     // --- Render Placeholder Rows for Edge Cases ---
     if num_cols == 0 && !grid_data.is_empty() {
         body.row(row_height, |mut row| { row.col(|ui| { ui.label("(No columns)"); }); });
         return false;
     }
     else if filtered_indices.is_empty() && !grid_data.is_empty() {
         body.row(row_height, |mut row| { row.col(|ui| { ui.label("(No rows match filter)"); }); });
         return false;
     }
     else if grid_data.is_empty() && registry.get_sheet(sheet_name).is_some() {
         body.row(row_height, |mut row| { row.col(|ui| { ui.label("(Sheet is empty)"); }); });
         return false;
     }
     else if registry.get_sheet(sheet_name).is_none() {
         // This case should ideally be handled by the caller preventing rendering,
         // but included as a fallback.
         body.row(row_height, |mut row| { row.col(|ui| { ui.label("Sheet missing"); }); });
         return false;
     }

    let num_filtered_rows = filtered_indices.len();

    // --- Main Row Rendering Loop ---
    body.rows(row_height, num_filtered_rows, |mut row: TableRow| {
        let filtered_row_index_in_list = row.index();
        // Get the original index from the pre-calculated filtered list
        let original_row_index = match filtered_indices.get(filtered_row_index_in_list) {
            Some(&idx) => idx,
            None => {
                error!("Filtered index out of bounds! List index: {}, List len: {}", filtered_row_index_in_list, filtered_indices.len());
                row.col(|ui| { ui.colored_label(egui::Color32::RED, "Err"); });
                return;
            }
        };

        // Safely get the current row using the original index
        if let Some(current_row) = grid_data.get(original_row_index) {
            // Consistency check: Ensure row has expected number of columns based on metadata
             if current_row.len() != num_cols {
                 row.col(|ui| {
                     ui.colored_label(
                         egui::Color32::RED,
                         format!("Row Len Err ({} vs {})", current_row.len(), num_cols)
                     );
                 });
                 warn!(
                     "Row length mismatch in sheet '{}', row {}: Expected {}, found {}",
                     sheet_name, original_row_index, num_cols, current_row.len()
                 );
                 return; // Skip rendering rest of this invalid row
             }

            // Render each column in the row
            for c_idx in 0..num_cols {
                row.col(|ui| {
                    // Safely get cell string and validator
                    if let Some(cell_string) = current_row.get(c_idx) {
                         let validator_opt = validators.get(c_idx).cloned().flatten();
                         let cell_id = egui::Id::new("cell").with(sheet_name).with(original_row_index).with(c_idx);

                        // --- Call the refactored edit widget ---
                        if let Some(new_value) = edit_cell_widget( // <-- CHANGED from ui_for_cell
                             ui,
                             cell_id,
                             cell_string,
                             &validator_opt,
                             registry,
                             state
                        ) {
                             // If the widget indicated a change, send an update event
                             cell_update_writer.send(UpdateCellEvent {
                                 sheet_name: sheet_name.to_string(),
                                 row_index: original_row_index,
                                 col_index: c_idx,
                                 new_value: new_value,
                             });
                        }
                    } else {
                        // This should ideally not happen if row length check passed
                        ui.colored_label(egui::Color32::RED, "Cell Err");
                        error!("Cell index {} out of bounds for row {} (len {}) in sheet '{}'", c_idx, original_row_index, current_row.len(), sheet_name);
                    }
                });
            } // End column loop
        } else {
            // This should not happen if filtered_indices are derived correctly
            row.col(|ui| { ui.colored_label(egui::Color32::RED, "Row Idx Err"); });
            error!("Original row index {} out of bounds (grid len {}) in sheet '{}'", original_row_index, grid_data.len(), sheet_name);
        }
    }); // End body.rows

    false // Return value is less important now
}