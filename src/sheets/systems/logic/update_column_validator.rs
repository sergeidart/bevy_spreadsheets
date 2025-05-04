// src/sheets/systems/logic/update_column_validator.rs
use bevy::prelude::*;
use std::collections::HashMap;
use crate::sheets::{
    definitions::{SheetMetadata, ColumnDataType, ColumnValidator},
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
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category; // &Option<String>
        let sheet_name = &event.sheet_name; // &String
        let col_index = event.column_index; // usize
        let new_validator_opt = &event.new_validator; // &Option<ColumnValidator>

        // --- Phase 1: Validation (Immutable Borrow) ---
        let validation_result: Result<(), String> = {
            let registry_immut = registry.as_ref();
            if let Some(sheet_data) = registry_immut.get_sheet(category, sheet_name) {
                if let Some(metadata) = &sheet_data.metadata { // metadata.category is Option<String>
                    if col_index < metadata.column_headers.len() {
                        if let Some(ref validator) = new_validator_opt {
                            match validator {
                                ColumnValidator::Basic(_) => Ok(()),
                                ColumnValidator::Linked { target_sheet_name, target_column_index } => { // target_sheet_name is &String

                                    // Find target sheet (any category)
// Find target sheet (any category)
let target_sheet_data_opt = registry_immut.iter_sheets()
    .find(|(_, name, _)| name.as_str() == target_sheet_name.as_str())
    .map(|(_, _, data)| data);

                                    // Prevent self-linking (check category too)
                                    // <<< --- FIX: Revert match arm to compare references --- >>>
                                    let is_same_category = match (&metadata.category, category) {
                                        (Some(ref meta_cat), Some(ref event_cat)) => meta_cat == event_cat, // Compare &String == &String
                                        (None, None) => true,
                                        _ => false,
                                    };

                                    // Compare String == String and usize == usize
                                    if *target_sheet_name == *sheet_name
                                        && *target_column_index == col_index
                                        && is_same_category
                                    {
                                         Err("Cannot link column to itself.".to_string())
                                    }
                                    // Check if target sheet exists
                                    else if let Some(target_sheet) = target_sheet_data_opt {
                                        if let Some(target_meta) = &target_sheet.metadata {
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
                                        Err(format!("Target sheet '{}' not found (in any category).", target_sheet_name))
                                    }
                                }
                            }
                        } else {
                            Ok(()) // Clearing validator is valid
                        }
                    } else {
                        Err(format!("Column index {} out of bounds ({} columns).", col_index, metadata.column_headers.len()))
                    }
                } else {
                    Err("Metadata missing.".to_string())
                }
            } else {
                 Err(format!("Sheet '{:?}/{}' not found.", category, sheet_name))
            }
        }; // End immutable validation block

        // --- Phase 2: Application (Mutable Borrow) ---
        match validation_result {
            Ok(()) => {
                let mut metadata_cache: Option<SheetMetadata> = None;
                if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
                    if let Some(metadata) = &mut sheet_data.metadata {
                        // Ensure consistency
                         if metadata.column_validators.len() != metadata.column_headers.len()
                            || metadata.column_types.len() != metadata.column_headers.len()
                            || metadata.column_filters.len() != metadata.column_headers.len()
                         {
                              error!("Metadata inconsistency detected in sheet '{:?}/{}' during validator update. Aborting update.", category, sheet_name);
                              feedback_writer.send(SheetOperationFeedback { message: format!("Internal Error: Metadata inconsistent in '{:?}/{}'. Update aborted.", category, sheet_name), is_error: true });
                              continue;
                         }

                        if col_index >= metadata.column_headers.len() {
                             error!("Column index {} out of bounds during validator update for '{:?}/{}' (mutable).", col_index, category, sheet_name);
                             continue;
                        }

                        // Update validator
                        let old_validator = std::mem::replace(&mut metadata.column_validators[col_index], new_validator_opt.clone());

                        // Update basic column type
                        let new_basic_type = match new_validator_opt {
                            Some(ColumnValidator::Basic(t)) => *t,
                            Some(ColumnValidator::Linked { .. }) => ColumnDataType::String,
                            None => ColumnDataType::String,
                        };
                        let old_type = std::mem::replace(&mut metadata.column_types[col_index], new_basic_type);

                        // Log and provide feedback
                        let change_msg = match (old_validator, new_validator_opt) {
                            (Some(old), Some(new)) => format!("Changed validator {:?} to {:?}. Type is now {:?}.", old, new, new_basic_type),
                            (Some(old), None) => format!("Cleared validator {:?}. Type is now {:?}.", old, new_basic_type),
                            (None, Some(new)) => format!("Set validator to {:?}. Type is now {:?}.", new, new_basic_type),
                            (None, None) => format!("Validator remains None. Type is {:?}.", new_basic_type),
                        };
                        let full_msg = format!("Updated validator for col {} of sheet '{:?}/{}'. {}", col_index + 1, category, sheet_name, change_msg);
                        info!("{}", full_msg);
                        if old_type != new_basic_type {
                             info!("Column basic data type changed from {:?} to {:?}.", old_type, new_basic_type);
                        }
                        feedback_writer.send(SheetOperationFeedback { message: full_msg, is_error: false });

                        metadata_cache = Some(metadata.clone());

                    }
                }

                if let Some(meta) = metadata_cache {
                     let key = (category.clone(), sheet_name.clone());
                     sheets_to_save.insert(key, meta);
                }
            }
            Err(err_msg) => {
                let msg = format!("Failed validator update for col {} of sheet '{:?}/{}': {}", col_index + 1, category, sheet_name, err_msg);
                error!("{}", msg);
                feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
            }
        }
    } // End event loop

    // --- Phase 3: Saving ---
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!("Validator updated for '{:?}/{}', triggering save.", cat, name);
            save_single_sheet(registry_immut, &metadata);
        }
    }
}