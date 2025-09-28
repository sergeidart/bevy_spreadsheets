// src/ui/elements/editor/editor_event_handling.rs
use super::main_editor::SheetEventWriters; // Assuming SheetEventWriters is made public or moved
use super::state::EditorWindowState;
use crate::sheets::definitions::RandomPickerMode;
use crate::sheets::{
    events::{RequestSheetRevalidation, SheetDataModifiedInRegistryEvent},
    resources::{SheetRegistry, SheetRenderCache},
};
use bevy::prelude::*;

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
        if !state.linked_column_cache.is_empty() || !state.linked_column_cache_normalized.is_empty()
        {
            let before = state.linked_column_cache.len();
            state
                .linked_column_cache
                .retain(|(target_sheet, _col_idx), _| target_sheet != &event.sheet_name);
            state
                .linked_column_cache_normalized
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
        if state.selected_category == event.category
            && state.selected_sheet_name.as_ref() == Some(&event.sheet_name)
        {
            debug!("editor_event_handling: Received SheetDataModifiedInRegistryEvent for current sheet '{:?}/{}'. Forcing filter recalc.", event.category, event.sheet_name);
            state.force_filter_recalculation = true;

            if state.request_scroll_to_new_row {
                if let Some(sheet_data) = registry.get_sheet(&event.category, &event.sheet_name) {
                    if !sheet_data.grid.is_empty() {
                        state.scroll_to_row_index = Some(0);
                        debug!(
                            "Scrolling to new row at top (index 0) for sheet '{:?}/{}'.",
                            event.category, event.sheet_name
                        );
                    }
                }
                state.request_scroll_to_new_row = false;
            }

            if render_cache
                .get_cell_data(&event.category, &event.sheet_name, 0, 0)
                .is_none()
                && registry
                    .get_sheet(&event.category, &event.sheet_name)
                    .map_or(false, |d| !d.grid.is_empty())
            {
                sheet_writers.revalidate.write(RequestSheetRevalidation {
                    category: event.category.clone(),
                    sheet_name: event.sheet_name.clone(),
                });
            }
        }
    }

    if initial_selected_category != &state.selected_category
        || initial_selected_sheet_name != &state.selected_sheet_name
    {
        debug!("Selected sheet or category changed by UI interaction.");
        state.reset_interaction_modes_and_selections();
        state.random_picker_needs_init = true;
        if let Some(sheet_name) = &state.selected_sheet_name {
            if render_cache
                .get_cell_data(&state.selected_category, sheet_name, 0, 0)
                .is_none()
                && registry
                    .get_sheet(&state.selected_category, sheet_name)
                    .map_or(false, |d| !d.grid.is_empty())
            {
                sheet_writers.revalidate.write(RequestSheetRevalidation {
                    category: state.selected_category.clone(),
                    sheet_name: sheet_name.clone(),
                });
            }

            // Initialize Random Picker UI from metadata or sensible defaults
            if let Some(sheet) = registry.get_sheet(&state.selected_category, sheet_name) {
                if let Some(meta) = &sheet.metadata {
                    let num_cols = meta.columns.len();
                    if let Some(rp) = &meta.random_picker {
                        state.random_picker_mode_is_complex =
                            matches!(rp.mode, RandomPickerMode::Complex);
                        state.random_simple_result_col =
                            rp.simple_result_col_index.min(num_cols.saturating_sub(1));
                        state.random_complex_result_col =
                            rp.complex_result_col_index.min(num_cols.saturating_sub(1));
                        state.random_complex_weight_col =
                            rp.weight_col_index.filter(|i| *i < num_cols);
                        state.random_complex_second_weight_col =
                            rp.second_weight_col_index.filter(|i| *i < num_cols);
                        // Restore dynamic weight columns list if available, otherwise fallback to legacy fields
                        state.random_picker_weight_columns.clear();
                        state.random_picker_weight_exponents.clear();
                        state.random_picker_weight_multipliers.clear();
                        if !rp.weight_columns.is_empty() {
                            // push only valid indices
                            for &ci in rp.weight_columns.iter() {
                                if ci < num_cols {
                                    state.random_picker_weight_columns.push(Some(ci));
                                }
                            }
                            let expected = state.random_picker_weight_columns.len();
                            // restore exponents or default to 1.0 for missing entries
                            if rp.weight_exponents.len() == expected {
                                state
                                    .random_picker_weight_exponents
                                    .extend_from_slice(&rp.weight_exponents[..expected]);
                            } else {
                                state.random_picker_weight_exponents.resize(expected, 1.0);
                            }
                            // restore multipliers or default to 1.0 for missing entries
                            if rp.weight_multipliers.len() == expected {
                                state
                                    .random_picker_weight_multipliers
                                    .extend_from_slice(&rp.weight_multipliers[..expected]);
                            } else {
                                state.random_picker_weight_multipliers.resize(expected, 1.0);
                            }
                        } else {
                            // legacy fallback: use legacy two-weight fields if present
                            if let Some(a) = rp.weight_col_index.filter(|i| *i < num_cols) {
                                state.random_picker_weight_columns.push(Some(a));
                            }
                            if let Some(b) = rp.second_weight_col_index.filter(|i| *i < num_cols) {
                                state.random_picker_weight_columns.push(Some(b));
                            }
                            let expected = state.random_picker_weight_columns.len();
                            state.random_picker_weight_exponents.resize(expected, 1.0);
                            state.random_picker_weight_multipliers.resize(expected, 1.0);
                        }
                        // ensure there is always at least one slot to allow quick UI addition
                        if state.random_picker_weight_columns.is_empty() {
                            state.random_picker_weight_columns.push(None);
                            state.random_picker_weight_exponents.push(1.0);
                            state.random_picker_weight_multipliers.push(1.0);
                        }
                        // Restore summarizer columns list
                        state.summarizer_selected_columns.clear();
                        if !rp.summarizer_columns.is_empty() {
                            for &ci in rp.summarizer_columns.iter() {
                                if ci < num_cols {
                                    state.summarizer_selected_columns.push(Some(ci));
                                }
                            }
                        } else if let Some(first) =
                            Some(rp.simple_result_col_index).filter(|_| true)
                        {
                            // default to single column if none stored
                            state.summarizer_selected_columns.push(Some(first));
                        }
                        if state.summarizer_selected_columns.is_empty() {
                            state.summarizer_selected_columns.push(None);
                        }
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
    }

    // Initialize Random Picker UI from metadata when flagged (e.g., on startup UI pass)
    if state.random_picker_needs_init {
        // Attempt to initialize only if registry has metadata for the selected sheet.
        // If metadata isn't loaded yet (startup race), leave the flag set so we retry next frame.
        let mut initialized = false;
        if let Some(sheet_name) = &state.selected_sheet_name {
            if let Some(sheet) = registry.get_sheet(&state.selected_category, sheet_name) {
                if let Some(meta) = &sheet.metadata {
                    let num_cols = meta.columns.len();
                    if let Some(rp) = &meta.random_picker {
                        state.random_picker_mode_is_complex =
                            matches!(rp.mode, RandomPickerMode::Complex);
                        state.random_simple_result_col =
                            rp.simple_result_col_index.min(num_cols.saturating_sub(1));
                        state.random_complex_result_col =
                            rp.complex_result_col_index.min(num_cols.saturating_sub(1));
                        state.random_complex_weight_col =
                            rp.weight_col_index.filter(|i| *i < num_cols);
                        state.random_complex_second_weight_col =
                            rp.second_weight_col_index.filter(|i| *i < num_cols);
                        // Restore dynamic weight columns + vectors (was previously missing in this init path)
                        state.random_picker_weight_columns.clear();
                        state.random_picker_weight_exponents.clear();
                        state.random_picker_weight_multipliers.clear();
                        if !rp.weight_columns.is_empty() {
                            for &ci in rp.weight_columns.iter() {
                                if ci < num_cols {
                                    state.random_picker_weight_columns.push(Some(ci));
                                }
                            }
                            let expected = state.random_picker_weight_columns.len();
                            if rp.weight_exponents.len() == expected {
                                state
                                    .random_picker_weight_exponents
                                    .extend_from_slice(&rp.weight_exponents[..expected]);
                            } else {
                                state.random_picker_weight_exponents.resize(expected, 1.0);
                            }
                            if rp.weight_multipliers.len() == expected {
                                state
                                    .random_picker_weight_multipliers
                                    .extend_from_slice(&rp.weight_multipliers[..expected]);
                            } else {
                                state.random_picker_weight_multipliers.resize(expected, 1.0);
                            }
                        } else {
                            // fallback to legacy two-column fields
                            if let Some(a) = rp.weight_col_index.filter(|i| *i < num_cols) {
                                state.random_picker_weight_columns.push(Some(a));
                            }
                            if let Some(b) = rp.second_weight_col_index.filter(|i| *i < num_cols) {
                                state.random_picker_weight_columns.push(Some(b));
                            }
                            let expected = state.random_picker_weight_columns.len();
                            state.random_picker_weight_exponents.resize(expected, 1.0);
                            state.random_picker_weight_multipliers.resize(expected, 1.0);
                        }
                        if state.random_picker_weight_columns.is_empty() {
                            state.random_picker_weight_columns.push(None);
                            state.random_picker_weight_exponents.push(1.0);
                            state.random_picker_weight_multipliers.push(1.0);
                        }
                        // Summarizer columns restore
                        state.summarizer_selected_columns.clear();
                        if !rp.summarizer_columns.is_empty() {
                            for &ci in rp.summarizer_columns.iter() {
                                if ci < num_cols {
                                    state.summarizer_selected_columns.push(Some(ci));
                                }
                            }
                        } else if let Some(first) =
                            Some(rp.simple_result_col_index).filter(|_| true)
                        {
                            state.summarizer_selected_columns.push(Some(first));
                        }
                        if state.summarizer_selected_columns.is_empty() {
                            state.summarizer_selected_columns.push(None);
                        }
                        debug!("Random Picker (init) restored: weights={}, summarizers={} for '{:?}/{}'", state.random_picker_weight_columns.iter().filter(|o| o.is_some()).count(), state.summarizer_selected_columns.iter().filter(|o| o.is_some()).count(), state.selected_category, sheet_name);
                    } else {
                        // Default: Simple with first column
                        state.random_picker_mode_is_complex = false;
                        state.random_simple_result_col = 0.min(num_cols.saturating_sub(1));
                        state.random_complex_result_col = 0.min(num_cols.saturating_sub(1));
                        state.random_complex_weight_col = None;
                        state.random_complex_second_weight_col = None;
                        // Ensure baseline vectors
                        state.random_picker_weight_columns.clear();
                        state.random_picker_weight_exponents.clear();
                        state.random_picker_weight_multipliers.clear();
                        state.random_picker_weight_columns.push(None);
                        state.random_picker_weight_exponents.push(1.0);
                        state.random_picker_weight_multipliers.push(1.0);
                        state.summarizer_selected_columns.clear();
                        state
                            .summarizer_selected_columns
                            .push(Some(state.random_simple_result_col));
                    }
                    state.random_picker_last_value.clear();
                    initialized = true;
                    debug!(
                        "Random Picker initialized from metadata for '{:?}/{}'.",
                        state.selected_category, sheet_name
                    );
                } else {
                    trace!(
                        "Random Picker init deferred: metadata not yet present for '{:?}/{}'.",
                        state.selected_category,
                        sheet_name
                    );
                }
            } else {
                trace!(
                    "Random Picker init deferred: sheet not found in registry for '{:?}/{}'.",
                    state.selected_category,
                    sheet_name
                );
            }
        } else {
            // No sheet selected -> nothing to initialize; clear the flag.
            initialized = true;
        }

        if initialized {
            state.random_picker_needs_init = false;
        }
    }
}
