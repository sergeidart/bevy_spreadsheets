// src/sheets/systems/logic/update_column_validator.rs
use bevy::prelude::*;
use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    events::{RequestUpdateColumnValidator, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};

/// Handles requests to update the validator (and derived type) for a specific column.
pub fn handle_update_column_validator(
    mut events: EventReader<RequestUpdateColumnValidator>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    let mut sheets_to_save = Vec::new();

    for event in events.read() {
        let sheet_name = &event.sheet_name;
        let col_index = event.column_index;
        let new_validator_opt = &event.new_validator; // Reference to Option<ColumnValidator>

        // --- Phase 1: Validation (Immutable Borrow) ---
        let validation_result: Result<(), String> = {
            let registry_immut = registry.as_ref(); // Immutable borrow for validation
            if let Some(sheet_data) = registry_immut.get_sheet(sheet_name) {
                if let Some(metadata) = &sheet_data.metadata {
                    if col_index < metadata.column_headers.len() {
                        // Validate the *new* validator definition if one is provided
                        if let Some(ref validator) = new_validator_opt {
                            match validator {
                                // Basic validator is always valid (type correctness checked later)
                                ColumnValidator::Basic(_) => Ok(()),
                                // Linked validator needs target validation
                                ColumnValidator::Linked { target_sheet_name, target_column_index } => {
                                    // Prevent self-linking
                                    if target_sheet_name == sheet_name && *target_column_index == col_index {
                                        Err("Cannot link column to itself.".to_string())
                                    }
                                    // Check if target sheet exists
                                    else if let Some(target_sheet) = registry_immut.get_sheet(target_sheet_name) {
                                        // Check if target sheet has metadata
                                        if let Some(target_meta) = &target_sheet.metadata {
                                            // Check if target column index is valid
                                            if *target_column_index < target_meta.column_headers.len() {
                                                Ok(()) // Target is valid
                                            } else {
                                                Err(format!(
                                                    "Target column index {} out of bounds for sheet '{}' ({} columns).",
                                                    target_column_index + 1, target_sheet_name, target_meta.column_headers.len()
                                                ))
                                            }
                                        } else {
                                            Err(format!("Target sheet '{}' is missing metadata.", target_sheet_name))
                                        }
                                    } else {
                                        Err(format!("Target sheet '{}' not found.", target_sheet_name))
                                    }
                                }
                            }
                        } else {
                            // Clearing the validator (new_validator is None) is always valid
                            Ok(())
                        }
                    } else {
                        Err(format!("Column index {} out of bounds ({} columns).", col_index, metadata.column_headers.len()))
                    }
                } else {
                    Err("Metadata missing.".to_string())
                }
            } else {
                Err("Sheet not found.".to_string())
            }
        }; // End immutable validation block

        // --- Phase 2: Application (Mutable Borrow) ---
        match validation_result {
            Ok(()) => {
                // Proceed with update using mutable borrow
                if let Some(sheet_data) = registry.get_sheet_mut(sheet_name) {
                    if let Some(metadata) = &mut sheet_data.metadata {
                        // Ensure internal consistency (lengths match headers) before modifying
                        // We rely on ensure_validator_consistency being called elsewhere (e.g., load/create)
                        // but a check here could be added for extra safety.
                         if metadata.column_validators.len() != metadata.column_headers.len()
                            || metadata.column_types.len() != metadata.column_headers.len()
                            || metadata.column_filters.len() != metadata.column_headers.len()
                         {
                              error!("Metadata inconsistency detected in sheet '{}' during validator update. Aborting update.", sheet_name);
                              feedback_writer.send(SheetOperationFeedback { message: format!("Internal Error: Metadata inconsistent in '{}'. Update aborted.", sheet_name), is_error: true });
                              continue; // Skip this event
                         }

                        // Check index again within mutable borrow context (should be fine)
                        if col_index >= metadata.column_headers.len() {
                             error!("Column index {} out of bounds during validator update (mutable).", col_index);
                             continue;
                        }

                        // Update validator
                        let old_validator = std::mem::replace(&mut metadata.column_validators[col_index], new_validator_opt.clone());

                        // Determine and update the basic column type based on the new validator
                        let new_basic_type = match new_validator_opt {
                            Some(ColumnValidator::Basic(t)) => *t,
                            Some(ColumnValidator::Linked { .. }) => ColumnDataType::String, // Linked columns interact as strings
                            None => ColumnDataType::String, // Default to String if validator is cleared
                        };
                        let old_type = std::mem::replace(&mut metadata.column_types[col_index], new_basic_type);

                        // Log and provide feedback
                        let change_msg = match (old_validator, new_validator_opt) {
                            (Some(old), Some(new)) => format!("Changed validator {:?} to {:?}. Type is now {:?}.", old, new, new_basic_type),
                            (Some(old), None) => format!("Cleared validator {:?}. Type is now {:?}.", old, new_basic_type),
                            (None, Some(new)) => format!("Set validator to {:?}. Type is now {:?}.", new, new_basic_type),
                            (None, None) => format!("Validator remains None. Type is {:?}.", new_basic_type), // Should not happen if type changed
                        };
                        let full_msg = format!("Updated validator for col {} of sheet '{}'. {}", col_index + 1, sheet_name, change_msg);
                        info!("{}", full_msg);
                        if old_type != new_basic_type {
                             info!("Column basic data type changed from {:?} to {:?}.", old_type, new_basic_type);
                        }
                        feedback_writer.send(SheetOperationFeedback { message: full_msg, is_error: false });

                        // Mark sheet for saving
                        if !sheets_to_save.contains(sheet_name) {
                            sheets_to_save.push(sheet_name.clone());
                        }
                    }
                    // else: Metadata missing (should not happen if validation passed)
                }
                // else: Sheet missing (should not happen if validation passed)
            }
            Err(err_msg) => {
                // Log validation errors
                let msg = format!("Failed validator update for col {} of sheet '{}': {}", col_index + 1, sheet_name, err_msg);
                error!("{}", msg);
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            }
        }
    } // End event loop

    // --- Phase 3: Saving ---
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref(); // Get immutable borrow for saving
        for sheet_name in sheets_to_save {
            info!("Validator updated for '{}', triggering save.", sheet_name);
            save_single_sheet(registry_immut, &sheet_name);
        }
    }
}