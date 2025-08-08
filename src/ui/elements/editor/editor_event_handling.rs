// src/ui/elements/editor/editor_event_handling.rs
use bevy::prelude::*;
use crate::sheets::{
    events::{SheetDataModifiedInRegistryEvent, RequestSheetRevalidation},
    resources::{SheetRegistry, SheetRenderCache},
};
use super::state::EditorWindowState;
use super::main_editor::SheetEventWriters; // Assuming SheetEventWriters is made public or moved
use crate::sheets::definitions::{RandomPickerMode};

#[allow(clippy::too_many_arguments)]
pub(super) fn process_editor_events_and_state(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    sheet_writers: &mut SheetEventWriters,
    sheet_data_modified_events: &mut EventReader<SheetDataModifiedInRegistryEvent>,
    initial_selected_category: &Option<String>,
    initial_selected_sheet_name: &Option<String>,
) {
    for event in sheet_data_modified_events.read() {
        // Any data modification may affect linked-column allowed values for this sheet.
        // Remove only the entries whose target_sheet_name matches the modified sheet to keep performance.
        if !state.linked_column_cache.is_empty() {
            let before = state.linked_column_cache.len();
            state
                .linked_column_cache
                .retain(|(target_sheet, _col_idx), _| target_sheet != &event.sheet_name);
            let after = state.linked_column_cache.len();
            if before != after {
                debug!(
                    "Pruned {} linked-column cache entries for modified sheet '{:?}/{}'.",
                    before - after,
                    event.category,
                    event.sheet_name
                );
            }
        }
        if state.selected_category == event.category && state.selected_sheet_name.as_ref() == Some(&event.sheet_name) {
            debug!("editor_event_handling: Received SheetDataModifiedInRegistryEvent for current sheet '{:?}/{}'. Forcing filter recalc.", event.category, event.sheet_name);
            state.force_filter_recalculation = true;

            if state.request_scroll_to_new_row {
                if let Some(sheet_data) = registry.get_sheet(&event.category, &event.sheet_name) {
                    if !sheet_data.grid.is_empty() {
                        state.scroll_to_row_index = Some(0);
                         debug!("Scrolling to new row at top (index 0) for sheet '{:?}/{}'.", event.category, event.sheet_name);
                    }
                }
                state.request_scroll_to_new_row = false;
            }

            if render_cache.get_cell_data(&event.category, &event.sheet_name, 0, 0).is_none()
                && registry.get_sheet(&event.category, &event.sheet_name).map_or(false, |d| !d.grid.is_empty()) {
                 sheet_writers.revalidate.write(RequestSheetRevalidation { category: event.category.clone(), sheet_name: event.sheet_name.clone() });
            }
        }
    }

    if initial_selected_category != &state.selected_category || initial_selected_sheet_name != &state.selected_sheet_name {
        debug!("Selected sheet or category changed by UI interaction.");
        state.reset_interaction_modes_and_selections();
        // Close AI config popup if open
        state.show_ai_rule_popup = false;
        state.random_picker_needs_init = true;
        if let Some(sheet_name) = &state.selected_sheet_name {
            if render_cache.get_cell_data(&state.selected_category, sheet_name, 0, 0).is_none()
                && registry.get_sheet(&state.selected_category, sheet_name).map_or(false, |d| !d.grid.is_empty()) {
                sheet_writers.revalidate.write(RequestSheetRevalidation { category: state.selected_category.clone(), sheet_name: sheet_name.clone() });
            }

            // Initialize Random Picker UI from metadata or sensible defaults
            if let Some(sheet) = registry.get_sheet(&state.selected_category, sheet_name) {
                if let Some(meta) = &sheet.metadata {
                    let num_cols = meta.columns.len();
                    if let Some(rp) = &meta.random_picker {
                        state.random_picker_mode_is_complex = matches!(rp.mode, RandomPickerMode::Complex);
                        state.random_simple_result_col = rp.simple_result_col_index.min(num_cols.saturating_sub(1));
                        state.random_complex_result_col = rp.complex_result_col_index.min(num_cols.saturating_sub(1));
                        state.random_complex_weight_col = rp.weight_col_index.filter(|i| *i < num_cols);
                        state.random_complex_second_weight_col = rp.second_weight_col_index.filter(|i| *i < num_cols);
                    } else {
                        // Default: Simple with first column
                        state.random_picker_mode_is_complex = false;
                        state.random_simple_result_col = 0.min(num_cols.saturating_sub(1));
                        state.random_complex_result_col = 0.min(num_cols.saturating_sub(1));
                        state.random_complex_weight_col = None;
                        state.random_complex_second_weight_col = None;
                    }
                    state.random_picker_last_value.clear();
                }
            }
        }
        state.force_filter_recalculation = true;
        state.ai_rule_popup_needs_init = true;
    }

    // Initialize Random Picker UI from metadata when flagged (e.g., on startup UI pass)
    if state.random_picker_needs_init {
        if let Some(sheet_name) = &state.selected_sheet_name {
            if let Some(sheet) = registry.get_sheet(&state.selected_category, sheet_name) {
                if let Some(meta) = &sheet.metadata {
                    let num_cols = meta.columns.len();
                    if let Some(rp) = &meta.random_picker {
                        state.random_picker_mode_is_complex = matches!(rp.mode, RandomPickerMode::Complex);
                        state.random_simple_result_col = rp.simple_result_col_index.min(num_cols.saturating_sub(1));
                        state.random_complex_result_col = rp.complex_result_col_index.min(num_cols.saturating_sub(1));
                        state.random_complex_weight_col = rp.weight_col_index.filter(|i| *i < num_cols);
                        state.random_complex_second_weight_col = rp.second_weight_col_index.filter(|i| *i < num_cols);
                    } else {
                        // Default: Simple with first column
                        state.random_picker_mode_is_complex = false;
                        state.random_simple_result_col = 0.min(num_cols.saturating_sub(1));
                        state.random_complex_result_col = 0.min(num_cols.saturating_sub(1));
                        state.random_complex_weight_col = None;
                        state.random_complex_second_weight_col = None;
                    }
                    state.random_picker_last_value.clear();
                }
            }
        }
        state.random_picker_needs_init = false;
    }
}