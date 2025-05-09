// src/ui/elements/editor/editor_event_handling.rs
use bevy::prelude::*;
use crate::sheets::{
    events::{SheetDataModifiedInRegistryEvent, RequestSheetRevalidation},
    resources::{SheetRegistry, SheetRenderCache},
};
use super::state::EditorWindowState;
use super::main_editor::SheetEventWriters; // Assuming SheetEventWriters is made public or moved

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
                 sheet_writers.revalidate.send(RequestSheetRevalidation { category: event.category.clone(), sheet_name: event.sheet_name.clone() });
            }
        }
    }

    if initial_selected_category != &state.selected_category || initial_selected_sheet_name != &state.selected_sheet_name {
        debug!("Selected sheet or category changed by UI interaction.");
        state.reset_interaction_modes_and_selections();
        if let Some(sheet_name) = &state.selected_sheet_name {
            if render_cache.get_cell_data(&state.selected_category, sheet_name, 0, 0).is_none()
                && registry.get_sheet(&state.selected_category, sheet_name).map_or(false, |d| !d.grid.is_empty()) {
                sheet_writers.revalidate.send(RequestSheetRevalidation { category: state.selected_category.clone(), sheet_name: sheet_name.clone() });
            }
        }
        state.force_filter_recalculation = true;
        state.ai_rule_popup_needs_init = true;
    }
}