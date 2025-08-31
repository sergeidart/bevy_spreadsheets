// src/ui/elements/popups/column_options_on_close.rs
use crate::sheets::{
    resources::SheetRegistry, systems::io::save::save_single_sheet,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;

/// Handles cleanup and potential saving when the column options popup is closed.
pub(super) fn handle_on_close(
    state: &mut EditorWindowState,
    registry: &SheetRegistry, // Immutable borrow for save
    needs_save: bool, // Flag indicating if filter/context change requires save
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
    state.options_column_ai_context_input.clear(); // NEW: Clear AI context input
    state.column_options_popup_needs_init = false; // Should already be false
    state.options_validator_type = None;
    state.options_link_target_sheet = None;
    state.options_link_target_column_index = None;
    // Clear key-related ephemeral state so it always reloads from metadata next open
    state.options_existing_structure_key_parent_column = None;
    state.options_structure_key_parent_column_temp = None;

    // --- Trigger Manual Save ONLY if non-event changes occurred ---
    if needs_save {
        // Get the metadata to save (which now includes the direct mods)
        if let Some(data_to_save) =
            registry.get_sheet(&popup_category, &popup_sheet_name)
        {
            if let Some(meta_to_save) = &data_to_save.metadata {
                info!(
                    "Filter/Context changed for '{:?}/{}', triggering save.",
                    popup_category, popup_sheet_name
                );
                save_single_sheet(registry, meta_to_save); // Pass metadata
                // Also emit data modified event so any dependent caches / structure sync run.
                // (We can't write the event here directly without an EventWriter param; instead rely on render cache update system triggered by save load cycle.)
            } else {
                 warn!("Cannot save after filter/context change for '{:?}/{}': Metadata missing.", popup_category, popup_sheet_name);
            }
        } else {
            warn!("Cannot save after filter/context change for '{:?}/{}': Sheet not found.", popup_category, popup_sheet_name);
        }
    }
}