use crate::ui::elements::editor::state::EditorWindowState;
use crate::sheets::resources::SheetRegistry;
use bevy::prelude::*;
use bevy_egui::egui;

use super::random_picker_ui::show_random_picker_window_ui;

pub fn show_random_picker_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
) {
    if !state.show_random_picker_panel { return; }

    let ui_result = show_random_picker_window_ui(ctx, state, &*registry);
    if ui_result.apply_clicked {
        // Persist into sheet metadata: store simple_result_col, complex_result_col same as simple for now
        let sheet_name = state.options_column_target_sheet.clone();
        if !sheet_name.is_empty() {
            // mutate metadata (create if missing), then clone to pass to save to avoid multiple borrows of registry
            if let Some(sheet) = registry.get_sheet_mut(&state.options_column_target_category, &sheet_name) {
                use crate::sheets::definitions::{RandomPickerSettings, SheetMetadata};
                // Ensure metadata exists for this sheet
                if sheet.metadata.is_none() {
                    // Create a conservative default metadata using current grid width if available
                    let num_cols = sheet.grid.first().map(|r| r.len()).unwrap_or(0);
                    let default_filename = format!("{}.json", sheet_name);
                    sheet.metadata = Some(SheetMetadata::create_generic(sheet_name.clone(), default_filename, num_cols, state.options_column_target_category.clone()));
                }
                if let Some(meta) = &mut sheet.metadata {
                    // Collect weight columns from dynamic list, stripping None
                    let weight_cols: Vec<usize> = state.random_picker_weight_columns.iter().filter_map(|o| *o).collect();
                    let summ_cols: Vec<usize> = state.summarizer_selected_columns.iter().filter_map(|o| *o).collect();
                    let settings = RandomPickerSettings {
                        mode: crate::sheets::definitions::RandomPickerMode::Simple,
                        // keep unified indices; legacy single-index fields intentionally omitted (set to None/0)
                        simple_result_col_index: state.random_simple_result_col,
                        complex_result_col_index: 0,
                        weight_col_index: None,
                        second_weight_col_index: None,
                        weight_columns: weight_cols.clone(),
                        weight_exponents: state.random_picker_weight_exponents.iter().cloned().take(weight_cols.len()).collect(),
                        weight_multipliers: state.random_picker_weight_multipliers.iter().cloned().take(weight_cols.len()).collect(),
                        summarizer_columns: summ_cols.clone(),
                    };
                    meta.random_picker = Some(settings.clone());
                    let meta_clone = meta.clone();
                    crate::sheets::systems::io::save::save_single_sheet(&*registry, &meta_clone);
                }
            }
        }
        state.show_random_picker_panel = false;
    }
    if ui_result.cancel_clicked || ui_result.close_via_x { state.show_random_picker_panel = false; }
}
