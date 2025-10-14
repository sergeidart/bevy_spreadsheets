// src/ui/elements/popups/column_options_validator/validator_validation.rs
// Validation logic for column validator configuration

use crate::ui::elements::editor::state::{EditorWindowState, ValidatorTypeChoice};

/// Checks if the current validator configuration in the state is valid for applying.
pub fn is_validator_config_valid(state: &EditorWindowState) -> bool {
    match state.options_validator_type {
        Some(ValidatorTypeChoice::Linked) => {
            state.options_link_target_sheet.is_some()
                && state.options_link_target_column_index.is_some()
        }
        Some(ValidatorTypeChoice::Structure) => true, // Always valid; confirmation handles risk
        Some(ValidatorTypeChoice::Basic) => true,     // Always valid
        None => false,
    }
}
