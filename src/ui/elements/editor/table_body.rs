// src/ui/elements/editor/table_body.rs
use crate::sheets::{
    definitions::{ColumnValidator, SheetMetadata},
    events::UpdateCellEvent,
    resources::{SheetRegistry, SheetRenderCache},
};
use crate::ui::common::edit_cell_widget;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState}; // AiModeState used for condition
use bevy::log::{debug, trace, error, warn};
use bevy::prelude::*;
use bevy_egui::egui;
use egui_extras::{TableBody, TableRow};
use std::hash::{Hash, Hasher};
use std::collections::HashSet;

fn calculate_filters_hash(filters: &Vec<Option<String>>) -> u64 {
    let mut s = std::collections::hash_map::DefaultHasher::new();
    filters.hash(&mut s);
    s.finish()
}

fn get_filtered_row_indices_internal(
    grid: &Vec<Vec<String>>,
    metadata: &SheetMetadata,
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


#[allow(clippy::too_many_arguments)]
pub fn sheet_table_body(
    mut body: TableBody,
    row_height: f32,
    category: &Option<String>,
    sheet_name: &str,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    mut cell_update_writer: EventWriter<UpdateCellEvent>,
    state: &mut EditorWindowState,
) -> bool {

    let sheet_data_ref = match registry.get_sheet(category, sheet_name) {
        Some(data) => data,
        None => {
            warn!("Sheet '{:?}/{}' not found in registry for table_body.", category, sheet_name);
            body.row(row_height, |mut row| { row.col(|ui| { ui.label("Sheet missing"); }); });
            return true;
        }
    };

    let metadata_ref = match &sheet_data_ref.metadata {
        Some(meta) => meta,
        None => {
            warn!("Sheet '{:?}/{}' found but metadata missing in table_body", category, sheet_name);
            body.row(row_height, |mut row| { row.col(|ui| { ui.label("Metadata missing"); }); });
            return true;
        }
    };

    let grid_data = &sheet_data_ref.grid;
    let num_cols = metadata_ref.columns.len();
    let validators: Vec<Option<ColumnValidator>> = metadata_ref.columns.iter().map(|c| c.validator.clone()).collect();

    let active_filters = metadata_ref.get_filters();
    let filters_hash = calculate_filters_hash(&active_filters);
    let cache_key = (category.clone(), sheet_name.to_string(), filters_hash);

    let filtered_indices = if !state.force_filter_recalculation && state.filtered_row_indices_cache.contains_key(&cache_key) {
        state.filtered_row_indices_cache.get(&cache_key).cloned().unwrap_or_else(|| {
             debug!("Cache key found but get failed for '{:?}/{}'. Recalculating.", category, sheet_name);
             let indices = get_filtered_row_indices_internal(grid_data, metadata_ref);
             state.filtered_row_indices_cache.insert(cache_key.clone(), indices.clone());
             indices
        })
    } else {
        debug!("Recalculating filtered indices for '{:?}/{}' (hash: {}, force_recalc: {})", category, sheet_name, filters_hash, state.force_filter_recalculation);
        let indices = get_filtered_row_indices_internal(grid_data, metadata_ref);
        state.filtered_row_indices_cache.insert(cache_key.clone(), indices.clone());
        if state.force_filter_recalculation {
            state.force_filter_recalculation = false;
        }
        indices
    };


    if num_cols == 0 && !grid_data.is_empty() {
        body.row(row_height, |mut row| { row.col(|ui| { ui.label("(No columns)"); }); });
        return false;
    } else if filtered_indices.is_empty() && !grid_data.is_empty() {
        body.row(row_height, |mut row| { row.col(|ui| { ui.label("(No rows match filter)"); }); });
        return false;
    } else if grid_data.is_empty() {
        body.row(row_height, |mut row| { row.col(|ui| { ui.label("(Sheet is empty)"); }); });
        return false;
    }


    let num_filtered_rows = filtered_indices.len();

    body.rows(
        row_height,
        num_filtered_rows,
        |mut ui_row: TableRow| {
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

            let current_row_actual_data_ref_opt = grid_data.get(original_row_index);

            if let Some(current_row_actual_data_ref) = current_row_actual_data_ref_opt {
                 if current_row_actual_data_ref.len() != num_cols {
                    ui_row.col(|ui| {
                        ui.colored_label(egui::Color32::RED, format!("Row Len Err ({} vs {})", current_row_actual_data_ref.len(), num_cols));
                    });
                    warn!("Row length mismatch in sheet '{:?}/{}', row {}: Expected {}, found {}", category, sheet_name, original_row_index, num_cols, current_row_actual_data_ref.len());
                    return;
                }

                for c_idx in 0..num_cols {
                    ui_row.col(|ui| {
                        // Enable checkboxes if AI mode is preparing OR delete row mode is active
                        let show_checkbox = (state.ai_mode == AiModeState::Preparing && !state.delete_row_mode_active) || state.delete_row_mode_active;
                        if c_idx == 0 && show_checkbox {
                            let mut is_selected = state.ai_selected_rows.contains(&original_row_index);
                            let response = ui.add(egui::Checkbox::without_text(&mut is_selected));
                            if response.changed() {
                                if is_selected { state.ai_selected_rows.insert(original_row_index); }
                                else { state.ai_selected_rows.remove(&original_row_index); }
                            }
                            ui.add_space(2.0); ui.separator(); ui.add_space(2.0);
                        }

                        let validator_opt_for_cell = validators.get(c_idx).cloned().flatten();
                        let cell_id = egui::Id::new("cell")
                            .with(category.as_deref().unwrap_or("root"))
                            .with(sheet_name)
                            .with(original_row_index)
                            .with(c_idx);

                        if let Some(new_value) = edit_cell_widget(
                            ui,
                            cell_id,
                            &validator_opt_for_cell,
                            category,
                            sheet_name,
                            original_row_index,
                            c_idx,
                            registry,
                            render_cache,
                            state,
                        ) {
                            cell_update_writer.send(UpdateCellEvent {
                                category: category.clone(),
                                sheet_name: sheet_name.to_string(),
                                row_index: original_row_index,
                                col_index: c_idx,
                                new_value: new_value,
                            });
                        }
                    });
                }
            } else {
                ui_row.col(|ui| { ui.colored_label(egui::Color32::RED, "Row Idx Err"); });
                error!("Original row index {} out of bounds for grid_data (len {}) in sheet '{:?}/{}' during table_body rendering.", original_row_index, grid_data.len(), category, sheet_name);
            }
        },
    );
    false
}