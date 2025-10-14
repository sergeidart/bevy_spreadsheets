// src/ui/elements/popups/column_options_validator/state_sync.rs
// Helper functions for syncing UI state with backend state

use crate::ui::elements::editor::state::EditorWindowState;

/// Syncs the existing structure key parent column from metadata to UI state
/// Returns true if state was updated
pub fn sync_existing_structure_key_state(
    state: &mut EditorWindowState,
    current_key_effective: Option<usize>,
) -> bool {
    if state.options_existing_structure_key_parent_column != current_key_effective {
        state.options_existing_structure_key_parent_column = current_key_effective;
        true
    } else {
        false
    }
}

/// Updates the pending structure key apply state when user changes key selection
pub fn update_pending_structure_key_apply(
    state: &mut EditorWindowState,
    new_key: Option<usize>,
) {
    state.pending_structure_key_apply = Some((
        state.options_column_target_category.clone(),
        state.options_column_target_sheet.clone(),
        state.options_column_target_index,
        new_key,
    ));
}

/// Syncs the new structure key parent column temp state
/// Returns true if state was updated
pub fn sync_new_structure_key_temp_state(
    state: &mut EditorWindowState,
    temp_choice: Option<usize>,
) -> bool {
    if temp_choice != state.options_structure_key_parent_column_temp {
        state.options_structure_key_parent_column_temp = temp_choice;
        true
    } else {
        false
    }
}
