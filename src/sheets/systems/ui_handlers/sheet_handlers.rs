// src/sheets/systems/ui_handlers/sheet_handlers.rs
use bevy::prelude::*;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, SheetInteractionState};

/// Handle sheet selection change
pub fn handle_sheet_selection(
    state: &mut EditorWindowState,
    new_sheet: Option<String>,
) {
    if state.selected_sheet_name != new_sheet {
        state.selected_sheet_name = new_sheet;
        if state.selected_sheet_name.is_none() {
            state.reset_interaction_modes_and_selections();
        }
        state.force_filter_recalculation = true;
        state.show_column_options_popup = false;
    }
}

/// Validate that the selected sheet still exists in the registry
pub fn validate_sheet_selection(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
) {
    if let Some(current_sheet_name) = state.selected_sheet_name.as_ref() {
        if !registry
            .get_sheet_names_in_category(&state.selected_category)
            .contains(current_sheet_name)
        {
            state.selected_sheet_name = None;
            state.reset_interaction_modes_and_selections();
            state.force_filter_recalculation = true;
            state.show_column_options_popup = false;
        }
    }
}

/// Handle rename sheet request
pub fn handle_rename_sheet_request(
    state: &mut EditorWindowState,
) {
    if let Some(ref name_to_rename) = state.selected_sheet_name {
        state.rename_target_category = state.selected_category.clone();
        state.rename_target_sheet = name_to_rename.clone();
        state.new_name_input = state.rename_target_sheet.clone();
        state.show_rename_popup = true;
    }
}

/// Handle delete sheet request
pub fn handle_delete_sheet_request(
    state: &mut EditorWindowState,
) {
    if let Some(ref name_to_delete) = state.selected_sheet_name {
        state.delete_target_category = state.selected_category.clone();
        state.delete_target_sheet = name_to_delete.clone();
        state.show_delete_confirm_popup = true;
    }
}

/// Handle new sheet request
pub fn handle_new_sheet_request(
    state: &mut EditorWindowState,
) {
    state.new_sheet_target_category = state.selected_category.clone();
    state.new_sheet_name_input.clear();
    state.show_new_sheet_popup = true;
}

/// Handle sheet picker expand/collapse toggle
pub fn handle_sheet_picker_toggle(
    state: &mut EditorWindowState,
) {
    state.sheet_picker_expanded = !state.sheet_picker_expanded;
}

/// Check if sheet management operations are allowed
pub fn can_manage_sheet(state: &EditorWindowState) -> bool {
    state.selected_sheet_name.is_some()
        && state.current_interaction_mode == SheetInteractionState::Idle
}

/// Handle sheet drag start
pub fn handle_sheet_drag_start(
    state: &mut EditorWindowState,
    sheet_name: String,
) {
    state.dragged_sheet = Some((state.selected_category.clone(), sheet_name));
}

/// Clear drag state
pub fn clear_drag_state(state: &mut EditorWindowState) {
    state.dragged_sheet = None;
}
