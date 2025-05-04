// src/ui/elements/editor/table_body.rs
use bevy::prelude::*;
use bevy_egui::egui;
use egui_extras::{TableBody, TableRow};

use crate::sheets::definitions::ColumnDataType;
use crate::sheets::resources::SheetRegistry;
use crate::ui::common::ui_for_cell;
use super::state::EditorWindowState; // For flagging save

/// Filters rows based on the "contains" logic for active column filters.
/// Returns a Vec containing the original indices of rows that pass ALL filters.
fn get_filtered_row_indices(
    grid: &Vec<Vec<String>>,
    filters: &[Option<String>], // Changed to slice &[Option<String>]
    _headers: &[String], // Keep for potential future debugging use
) -> Vec<usize> {
    if filters.iter().all(Option::is_none) {
        // Optimization: If no filters are active, return all indices
        return (0..grid.len()).collect();
    }

    (0..grid.len())
        .filter(|&row_idx| {
            // Get the current row safely
            if let Some(row) = grid.get(row_idx) {
                // Check if this row passes ALL active filters
                filters.iter().enumerate().all(|(col_idx, filter_opt)| {
                    match filter_opt {
                        // Filter is active and not empty
                        Some(filter_text) if !filter_text.is_empty() => {
                             // Get the cell text for this column safely
                             row.get(col_idx)
                                 // Perform case-insensitive contains check
                                .map_or(false, |cell_text| cell_text.to_lowercase().contains(&filter_text.to_lowercase()))
                        }
                        // No filter for this column, or filter is empty string: always passes
                        _ => true,
                    }
                })
            } else {
                // Row index out of bounds (shouldn't happen ideally)
                warn!("Row index {} out of bounds during filtering.", row_idx);
                false
            }
        })
        .collect() // Collect the original indices that passed
}

/// Renders the body rows for the sheet table, handling filtering and cell editing.
/// Returns true if any cell was changed during this frame.
pub fn sheet_table_body(
    body: TableBody, // egui_extras table body
    row_height: f32, // Pass calculated row height in
    sheet_name: &str,
    headers: &[String], // Needed for filtering context and num_cols
    column_types: &[ColumnDataType],
    filters: &[Option<String>],
    registry: &mut SheetRegistry, // Mutable registry for cell edits
    state: &mut EditorWindowState, // Mutable state to flag saves
) -> bool {
    let mut change_occurred = false;
    let mut body = body; // Make mutable for use

    // --- Get grid data and apply filtering ---
    let (filtered_indices, num_cols) = if let Some(sheet_data) = registry.get_sheet(sheet_name) {
        (
            // Pass filters as slice
            get_filtered_row_indices(&sheet_data.grid, filters, headers),
            sheet_data.metadata.as_ref().map_or(0, |m| m.column_headers.len()),
        )
    } else {
        // Sheet disappeared? Render empty or error message
        body.row(row_height, |mut row| { // Use passed row_height
            row.col(|ui| {
                ui.label("Sheet data unavailable.");
            });
        });
        return false; // No changes possible
    };

    let num_filtered_rows = filtered_indices.len();

    // --- Render Filtered Rows ---
    // Borrow registry mutably *once* if possible, for the duration of the loop
    if let Some(sheet_data_mut) = registry.get_sheet_mut(sheet_name) {
        body.rows(row_height, num_filtered_rows, |mut row: TableRow| { // Use passed row_height and num_filtered_rows
            let filtered_row_index_in_list = row.index(); // 0..num_filtered_rows-1

            // Map back to original grid index
            let original_row_index = match filtered_indices.get(filtered_row_index_in_list) {
                Some(&idx) => idx,
                None => {
                    error!("Filtered index {} out of bounds!", filtered_row_index_in_list);
                    row.col(|ui| { ui.label("Error: Invalid Filtered Index"); });
                    return;
                }
            };

            // Access the mutable grid using the original index
            if original_row_index < sheet_data_mut.grid.len() {
                // Resize row if column count mismatch (modifies underlying data)
                if sheet_data_mut.grid[original_row_index].len() != num_cols {
                    sheet_data_mut.grid[original_row_index].resize(num_cols, String::new());
                    change_occurred = true; // Mark change if resized
                }

                if num_cols == 0 {
                    row.col(|ui| { ui.label("(No columns defined)"); });
                    return;
                }

                // Borrow the specific row mutably once
                let current_row_mut = &mut sheet_data_mut.grid[original_row_index];
                for c_idx in 0..num_cols {
                    row.col(|ui| {
                        // Borrow the cell string mutably
                        if let Some(cell_string_mut) = current_row_mut.get_mut(c_idx) {
                             if let Some(col_type) = column_types.get(c_idx) {
                                 // Generate unique ID using original row index
                                 let cell_id = egui::Id::new("cell")
                                     .with(sheet_name)
                                     .with(original_row_index) // Use original index for ID stability
                                     .with(c_idx);
                                 if ui_for_cell(ui, cell_id, cell_string_mut, *col_type) {
                                     change_occurred = true; // Mark change if cell edited
                                     // Flag save immediately when a cell changes
                                     state.sheet_needs_save = true;
                                     state.sheet_to_save = sheet_name.to_string();
                                 }
                             } else { ui.label("Error: Type invalid");}
                        } else { ui.label("Error: Index invalid"); }
                    });
                } // End column loop
            } else {
                row.col(|ui| { ui.label("Error: Original Row index invalid"); });
            }
        }); // End body.rows
    } else {
        // Sheet disappeared between initial check and mutable borrow
        body.row(row_height, |mut row| { // Use passed row_height
            row.col(|ui| {
                ui.label("Sheet data unavailable (mut).");
            });
        });
    }

    change_occurred
}