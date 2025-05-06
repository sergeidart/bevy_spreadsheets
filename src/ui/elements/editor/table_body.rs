// src/ui/elements/editor/table_body.rs
use crate::sheets::{
    definitions::{ColumnValidator, SheetMetadata}, // Added SheetMetadata
    events::UpdateCellEvent,
    resources::SheetRegistry,
};
use crate::ui::common::edit_cell_widget;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState};
use bevy::prelude::*;
use bevy_egui::egui;
use egui_extras::{TableBody, TableRow};
use std::hash::{Hash, Hasher}; // For hashing filters

// Helper to calculate hash for filter state
fn calculate_filters_hash(filters: &Vec<Option<String>>) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    filters.hash(&mut s);
    s.finish()
}


fn get_filtered_row_indices_internal( // Renamed to avoid conflict, now internal
    grid: &Vec<Vec<String>>, // Takes reference
    metadata: &SheetMetadata, // Takes reference
) -> Vec<usize> {
    let filters: Vec<Option<String>> =
        metadata.columns.iter().map(|c| c.filter.clone()).collect();
    if filters.iter().all(Option::is_none) {
        return (0..grid.len()).collect();
    }

    (0..grid.len())
        .filter(|&row_idx| {
            if let Some(row) = grid.get(row_idx) {
                filters.iter().enumerate().all(|(col_idx, filter_opt)| {
                    match filter_opt {
                        Some(filter_text) if !filter_text.is_empty() => row
                            .get(col_idx)
                            .map_or(false, |cell_text| {
                                cell_text
                                    .to_lowercase()
                                    .contains(&filter_text.to_lowercase())
                            }),
                        _ => true,
                    }
                })
            } else {
                false
            } 
        })
        .collect()
}

/// Renders the body rows for the sheet table, handling filtering and cell editing via Events.
pub fn sheet_table_body(
    mut body: TableBody, // Make mutable for direct use
    row_height: f32,
    category: &Option<String>, 
    sheet_name: &str, // Is &String
    registry: &SheetRegistry, 
    mut cell_update_writer: EventWriter<UpdateCellEvent>, 
    state: &mut EditorWindowState, 
) -> bool { // Return value is less important
    
    // --- Get sheet data reference ---
    let sheet_data_ref = match registry.get_sheet(category, sheet_name) {
        Some(data) => data,
        None => {
            warn!("Sheet '{:?}/{}' not found in registry for table_body.", category, sheet_name);
            body.row(row_height, |mut row| { row.col(|ui| { ui.label("Sheet missing"); }); });
            return false;
        }
    };

    let metadata_ref = match &sheet_data_ref.metadata {
        Some(meta) => meta,
        None => {
            warn!("Sheet '{:?}/{}' found but metadata missing in table_body", category, sheet_name);
            body.row(row_height, |mut row| { row.col(|ui| { ui.label("Metadata missing"); }); });
            return false;
        }
    };

    let grid_data = &sheet_data_ref.grid; // Reference to grid
    let num_cols = metadata_ref.columns.len();
    let validators: Vec<Option<ColumnValidator>> = metadata_ref.columns.iter().map(|c| c.validator.clone()).collect();


    // --- Filtered Indices Cache Logic ---
    let active_filters = metadata_ref.get_filters(); // Clones Vec<Option<String>>
    let filters_hash = calculate_filters_hash(&active_filters);
    let cache_key = (category.clone(), sheet_name.to_string(), filters_hash);

    let filtered_indices = if !state.force_filter_recalculation && state.filtered_row_indices_cache.contains_key(&cache_key) {
        if let Some(cached_indices) = state.filtered_row_indices_cache.get(&cache_key) {
            trace!("Using cached filtered indices for '{:?}/{}' (hash: {})", category, sheet_name, filters_hash);
            cached_indices.clone() // Clone the Vec<usize>
        } else {
            // Should not happen if contains_key is true, but defensive
            debug!("Cache key found but get failed for '{:?}/{}'. Recalculating.", category, sheet_name);
            let indices = get_filtered_row_indices_internal(grid_data, metadata_ref);
            state.filtered_row_indices_cache.insert(cache_key.clone(), indices.clone());
            indices
        }
    } else {
        debug!("Recalculating filtered indices for '{:?}/{}' (hash: {}, force_recalc: {})", category, sheet_name, filters_hash, state.force_filter_recalculation);
        let indices = get_filtered_row_indices_internal(grid_data, metadata_ref);
        state.filtered_row_indices_cache.insert(cache_key.clone(), indices.clone());
        // Reset the force flag after recalculation
        if state.force_filter_recalculation {
            state.force_filter_recalculation = false;
        }
        indices
    };
    // --- End Filtered Indices Cache Logic ---


    if num_cols == 0 && !grid_data.is_empty() {
        body.row(row_height, |mut row| { row.col(|ui| { ui.label("(No columns)"); }); });
        return false;
    } else if filtered_indices.is_empty() && !grid_data.is_empty() {
        body.row(row_height, |mut row| { row.col(|ui| { ui.label("(No rows match filter)"); }); });
        return false;
    } else if grid_data.is_empty() { // Simplified: registry.get_sheet already confirmed sheet exists
        body.row(row_height, |mut row| { row.col(|ui| { ui.label("(Sheet is empty)"); }); });
        return false;
    }

    let num_filtered_rows = filtered_indices.len();

    body.rows(
        row_height,
        num_filtered_rows,
        |mut ui_row: TableRow| { // Renamed to ui_row to avoid conflict with current_row
            let filtered_row_index_in_list = ui_row.index();
            let original_row_index =
                match filtered_indices.get(filtered_row_index_in_list) {
                    Some(&idx) => idx,
                    None => {
                        error!("Filtered index out of bounds! List index: {}, List len: {}", filtered_row_index_in_list, filtered_indices.len());
                        ui_row.col(|ui| { ui.colored_label(egui::Color32::RED, "Err"); });
                        return;
                    }
                };

            // Get a reference to the current row data from the grid_data reference
            if let Some(current_row_ref) = grid_data.get(original_row_index) {
                if current_row_ref.len() != num_cols {
                    ui_row.col(|ui| {
                        ui.colored_label(egui::Color32::RED, format!("Row Len Err ({} vs {})", current_row_ref.len(), num_cols));
                    });
                    warn!("Row length mismatch in sheet '{:?}/{}', row {}: Expected {}, found {}", category, sheet_name, original_row_index, num_cols, current_row_ref.len());
                    return; 
                }
                for c_idx in 0..num_cols {
                    ui_row.col(|ui| {
                        if c_idx == 0 && state.ai_mode == AiModeState::Preparing {
                            let mut is_selected = state.ai_selected_rows.contains(&original_row_index);
                            let response = ui.add(egui::Checkbox::without_text(&mut is_selected));
                            if response.changed() {
                                if is_selected {
                                    state.ai_selected_rows.insert(original_row_index);
                                    trace!("Selected row: {}", original_row_index);
                                } else {
                                    state.ai_selected_rows.remove(&original_row_index);
                                    trace!("Deselected row: {}", original_row_index);
                                }
                            }
                            ui.add_space(2.0);
                            ui.separator(); 
                            ui.add_space(2.0);
                        }
                        // Get cell_string as a reference
                        if let Some(cell_string_ref) = current_row_ref.get(c_idx) {
                            let validator_opt = validators.get(c_idx).cloned().flatten();
                            let cell_id = egui::Id::new("cell")
                                .with(category.as_deref().unwrap_or("root"))
                                .with(sheet_name) // sheet_name is &str
                                .with(original_row_index)
                                .with(c_idx);
                            
                            // edit_cell_widget takes &str for current_cell_string
                            if let Some(new_value) = edit_cell_widget(
                                ui,
                                cell_id,
                                cell_string_ref, // Pass reference
                                &validator_opt,
                                registry, 
                                state,    
                            ) {
                                cell_update_writer.send(UpdateCellEvent {
                                    category: category.clone(), 
                                    sheet_name: sheet_name.to_string(), // Convert &str to String
                                    row_index: original_row_index,
                                    col_index: c_idx,
                                    new_value: new_value,
                                });
                            }
                        } else {
                            ui.colored_label(egui::Color32::RED, "Cell Err");
                            error!("Cell index {} out of bounds for row {} (len {}) in sheet '{:?}/{}'", c_idx, original_row_index, current_row_ref.len(), category, sheet_name);
                        }
                    });
                } 
            } else {
                ui_row.col(|ui| { ui.colored_label(egui::Color32::RED, "Row Idx Err"); });
                error!("Original row index {} out of bounds (grid len {}) in sheet '{:?}/{}'", original_row_index, grid_data.len(), category, sheet_name);
            }
        },
    ); 
    false 
}