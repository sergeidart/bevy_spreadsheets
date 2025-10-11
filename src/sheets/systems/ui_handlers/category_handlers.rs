// src/sheets/systems/ui_handlers/category_handlers.rs
use bevy::prelude::*;
use crate::sheets::events::RequestMoveSheetToCategory;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

/// Handle category selection change
pub fn handle_category_selection(
    state: &mut EditorWindowState,
    new_category: Option<String>,
) {
    if state.selected_category != new_category {
        state.selected_category = new_category;
        state.selected_sheet_name = None;
        state.reset_interaction_modes_and_selections();
        state.force_filter_recalculation = true;
    }
}

/// Handle sheet drop onto category (returns true if drop was consumed)
pub fn handle_sheet_drop_to_category(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    target_category: Option<String>,
    move_event_writer: &mut EventWriter<RequestMoveSheetToCategory>,
) -> bool {
    if let Some((from_cat, sheet)) = state.dragged_sheet.take() {
        // Only drop if valid: destination differs and no name conflict
        if from_cat != target_category && registry.get_sheet(&target_category, &sheet).is_none() {
            move_event_writer.write(RequestMoveSheetToCategory {
                from_category: from_cat,
                sheet_name: sheet.clone(),
                to_category: target_category.clone(),
            });
            state.selected_category = target_category;
            state.selected_sheet_name = Some(sheet);
            state.reset_interaction_modes_and_selections();
            state.force_filter_recalculation = true;
            return true;
        }
    }
    false
}

/// Handle rename category request
pub fn handle_rename_category_request(
    state: &mut EditorWindowState,
) {
    state.rename_target_category = state.selected_category.clone();
    state.rename_target_sheet.clear();
    state.new_name_input = state.rename_target_category.clone().unwrap_or_default();
    state.show_rename_popup = true;
}

/// Handle delete category request
pub fn handle_delete_category_request(
    state: &mut EditorWindowState,
) {
    state.delete_category_name = state.selected_category.clone();
    state.show_delete_category_confirm_popup = true;
}

/// Handle new category request
pub fn handle_new_category_request(
    state: &mut EditorWindowState,
) {
    state.show_new_category_popup = true;
    state.new_category_name_input.clear();
}

/// Handle category picker expand/collapse toggle
pub fn handle_category_picker_toggle(
    state: &mut EditorWindowState,
) {
    state.category_picker_expanded = !state.category_picker_expanded;
}

/// Handle AI output panel visibility toggle
pub fn handle_ai_output_panel_toggle(
    state: &mut EditorWindowState,
) {
    state.ai_output_panel_visible = !state.ai_output_panel_visible;
}

/// Check if a drop operation is valid (no conflict and different category)
pub fn is_drop_valid(
    from_cat: &Option<String>,
    target_category: &Option<String>,
    sheet_name: &str,
    registry: &SheetRegistry,
) -> bool {
    from_cat != target_category && registry.get_sheet(target_category, sheet_name).is_none()
}

/// Handle drop on target widget (returns true if drop consumed and should continue/return)
pub fn handle_drop_on_target(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    target_category: Option<String>,
    move_event_writer: &mut EventWriter<RequestMoveSheetToCategory>,
    rect_contains_pointer: bool,
    primary_released: bool,
) -> bool {
    if primary_released && rect_contains_pointer {
        if handle_sheet_drop_to_category(state, registry, target_category, move_event_writer) {
            return true;
        }
    }
    false
}
