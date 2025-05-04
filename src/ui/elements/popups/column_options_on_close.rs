// src/ui/elements/popups/column_options_on_close.rs
use bevy::prelude::*;
use crate::sheets::{
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use crate::ui::elements::editor::state::EditorWindowState;

/// Handles cleanup and potential saving when the column options popup is closed.
pub(super) fn handle_on_close(
    state: &mut EditorWindowState,
    registry: &SheetRegistry, // Immutable borrow for save
    needs_save: bool, // Flag indicating if filter change requires save
) {
    let popup_category = state.options_column_target_category.clone();
    let popup_sheet_name = state.options_column_target_sheet.clone();

    // --- Clear State ---
    state.show_column_options_popup = false;
    state.options_column_target_category = None;
    state.options_column_target_sheet.clear();
    state.options_column_target_index = 0;
    state.options_column_rename_input.clear();
    state.options_column_filter_input.clear();
    state.column_options_popup_needs_init = false; // Should already be false
    state.options_validator_type = None;
    state.options_link_target_sheet = None;
    state.options_link_target_column_index = None;

    // --- Trigger Manual Save ONLY if ONLY the filter changed ---
    if needs_save {
        // Get the metadata to save
        if let Some(data_to_save) = registry.get_sheet(&popup_category, &popup_sheet_name) {
            if let Some(meta_to_save) = &data_to_save.metadata {
                info!(
                    "Filter changed for '{:?}/{}', triggering save.",
                    popup_category, popup_sheet_name
                );
                save_single_sheet(registry, meta_to_save); // Pass metadata
            }
        }
    }
}