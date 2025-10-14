// src/ui/elements/popups/column_options_validator/validator_apply.rs
// Logic for applying validator updates and sending events

use crate::sheets::{
    definitions::ColumnValidator,
    events::RequestUpdateColumnValidator,
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::{EditorWindowState, ValidatorTypeChoice};
use bevy::prelude::*;

/// Checks the current state and sends a validator update event if needed.
/// Returns true if the action was successful (or no change needed), false if validation failed.
pub fn apply_validator_update(
    state: &EditorWindowState,
    registry: &SheetRegistry, // Immutable borrow is sufficient here
    column_validator_writer: &mut EventWriter<RequestUpdateColumnValidator>,
) -> bool {
    let category = &state.options_column_target_category;
    let sheet_name = &state.options_column_target_sheet;
    let col_index = state.options_column_target_index;

    // --- CORRECTED: Get current validator from ColumnDefinition ---
    let current_validator = registry
        .get_sheet(category, sheet_name)
        .and_then(|s| s.metadata.as_ref())
        .and_then(|m| m.columns.get(col_index)) // Get the ColumnDefinition Option
        .and_then(|col_def| col_def.validator.clone()); // Clone the Option<Validator> inside

    // Construct New Validator
    let (new_validator, validation_ok) = match state.options_validator_type {
        Some(ValidatorTypeChoice::Basic) => (
            Some(ColumnValidator::Basic(state.options_basic_type_select)),
            true,
        ),
        Some(ValidatorTypeChoice::Linked) => {
            if let (Some(ts), Some(tc)) = (
                state.options_link_target_sheet.as_ref(),
                state.options_link_target_column_index,
            ) {
                (
                    Some(ColumnValidator::Linked {
                        target_sheet_name: ts.clone(),
                        target_column_index: tc,
                    }),
                    true,
                )
            } else {
                warn!("Validator update failed: Linked target invalid.");
                (None, false) // Action failed
            }
        }
        Some(ValidatorTypeChoice::Structure) => (Some(ColumnValidator::Structure), true),
        None => {
            warn!("Validator update failed: Invalid internal state.");
            (None, false) // Action failed
        }
    };

    if !validation_ok {
        return false; // Return early if validation failed
    }

    let validator_changed = current_validator != new_validator;

    // Send Validator Update Event if changed
    if validator_changed {
        info!(
            "Validator change detected for col {} of '{:?}/{}'. Sending update event.",
            col_index + 1,
            category,
            sheet_name
        );
        if matches!(new_validator, Some(ColumnValidator::Structure)) {
            // If sheet currently has no structure validator (we're creating) try to use temp creation key.
            if !matches!(current_validator, Some(ColumnValidator::Structure)) {
                let _ = state.options_structure_key_parent_column_temp; // selection carried via event below
            }
        }
        column_validator_writer.write(RequestUpdateColumnValidator {
            category: category.clone(),
            sheet_name: sheet_name.clone(),
            column_index: col_index,
            new_validator: new_validator.clone(),
            structure_source_columns: if matches!(new_validator, Some(ColumnValidator::Structure)) {
                let sources: Vec<usize> = state
                    .options_structure_source_columns
                    .iter()
                    .filter_map(|o| *o)
                    .collect();
                if sources.is_empty() {
                    None
                } else {
                    Some(sources)
                }
            } else {
                None
            },
            key_parent_column_index: if matches!(new_validator, Some(ColumnValidator::Structure)) {
                state.options_structure_key_parent_column_temp
            } else {
                None
            },
            original_self_validator: if matches!(new_validator, Some(ColumnValidator::Structure)) {
                current_validator.clone()
            } else {
                None
            },
        });
        // NOTE: key_parent_col captured; actual persistence must be handled by downstream event handler updating metadata, which should look at pending_structure_key_apply or similar. If such mechanism exists, it can be extended; for now we rely on separate pending_structure_key_apply logic elsewhere if needed.
    } else {
        trace!(
            "Validator unchanged for col {} of '{:?}/{}'.",
            col_index + 1,
            category,
            sheet_name
        );
    }

    true // Action successful or no change needed
}
