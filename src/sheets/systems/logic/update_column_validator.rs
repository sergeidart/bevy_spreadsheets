// src/sheets/systems/logic/update_column_validator.rs
use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator, SheetMetadata}, // Added ColumnDefinition
    events::{RequestUpdateColumnValidator, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use std::collections::HashMap;

/// Handles requests to update the validator (and derived type) for a specific column.
pub fn handle_update_column_validator(
    mut events: EventReader<RequestUpdateColumnValidator>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> =
        HashMap::new();

    for event in events.read() {
        let category = &event.category; // &Option<String>
        let sheet_name = &event.sheet_name; // &String
        let col_index = event.column_index; // usize
        let new_validator_opt = &event.new_validator; // &Option<ColumnValidator>

        // --- Phase 1: Validation (Immutable Borrow) ---
        let validation_result: Result<(), String> = {
            let registry_immut = registry.as_ref();
            if let Some(sheet_data) = registry_immut.get_sheet(category, sheet_name)
            {
                if let Some(metadata) = &sheet_data.metadata {
                    // --- CORRECTED: Check bounds using columns.len() ---
                    if col_index < metadata.columns.len() {
                        if let Some(ref validator) = new_validator_opt {
                            match validator {
                                ColumnValidator::Basic(_) => Ok(()),
                                ColumnValidator::Linked {
                                    target_sheet_name,
                                    target_column_index,
                                } => {
                                    // Find target sheet (any category)
                                    let target_sheet_data_opt = registry_immut
                                        .iter_sheets()
                                        .find(|(_, name, _)| {
                                            name.as_str() == target_sheet_name.as_str()
                                        })
                                        .map(|(_, _, data)| data);

                                    // Prevent self-linking (check category too)
                                    let is_same_category = match (&metadata.category, category) {
                                        (Some(ref meta_cat), Some(ref event_cat)) => meta_cat == event_cat,
                                        (None, None) => true,
                                        _ => false,
                                    };

                                    if *target_sheet_name == *sheet_name
                                        && *target_column_index == col_index
                                        && is_same_category
                                    {
                                        Err("Cannot link column to itself.".to_string())
                                    }
                                    // Check if target sheet exists
                                    else if let Some(target_sheet) = target_sheet_data_opt {
                                        if let Some(target_meta) = &target_sheet.metadata {
                                            // --- CORRECTED: Check target bounds using columns.len() ---
                                            if *target_column_index < target_meta.columns.len() {
                                                Ok(()) // Target is valid
                                            } else {
                                                Err(format!(
                                                    "Target column index {} out of bounds for sheet '{}' ({} columns).",
                                                    target_column_index, // 0-based index internally
                                                    target_sheet_name,
                                                    target_meta.columns.len() // Use columns.len()
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
                        // --- CORRECTED: Use columns.len() in error message ---
                        Err(format!(
                            "Column index {} out of bounds ({} columns).",
                            col_index,
                            metadata.columns.len()
                        ))
                    }
                } else {
                    Err("Metadata missing.".to_string())
                }
            } else {
                Err(format!(
                    "Sheet '{:?}/{}' not found.",
                    category, sheet_name
                ))
            }
        }; // End immutable validation block

        // --- Phase 2: Application (Mutable Borrow) ---
        match validation_result {
            Ok(()) => {
                let mut metadata_cache: Option<SheetMetadata> = None;
                if let Some(sheet_data) =
                    registry.get_sheet_mut(category, sheet_name)
                {
                    if let Some(metadata) = &mut sheet_data.metadata {
                        // Basic bounds check again for safety
                        if col_index >= metadata.columns.len() {
                             error!("Column index {} out of bounds during validator update for '{:?}/{}' (mutable).", col_index, category, sheet_name);
                             continue; // Should not happen if validation passed
                        }

                        // Get mutable access to the specific column definition
                        let column_def = &mut metadata.columns[col_index];

                        // Update validator
                        let old_validator = std::mem::replace(
                            &mut column_def.validator, // Access validator in ColumnDef
                            new_validator_opt.clone(),
                        );

                        // Determine the new basic type based on the new validator
                        let new_basic_type = match new_validator_opt {
                            Some(ColumnValidator::Basic(t)) => *t,
                            Some(ColumnValidator::Linked { .. }) => ColumnDataType::String, // Linked columns use String type for UI/internal rep
                            None => ColumnDataType::String, // Default if cleared
                        };

                        // Update basic column type directly in ColumnDef
                        let old_type = std::mem::replace(
                            &mut column_def.data_type, // Access data_type in ColumnDef
                            new_basic_type,
                        );

                        // Optional: Call ensure_type_consistency if logic becomes more complex
                        // column_def.ensure_type_consistency();

                        // Log and provide feedback
                        let change_msg = match (old_validator, new_validator_opt) {
                            (Some(old), Some(new)) => format!(
                                "Changed validator {:?} to {:?}. Type is now {:?}.",
                                old, new, new_basic_type
                            ),
                            (Some(old), None) => format!(
                                "Cleared validator {:?}. Type is now {:?}.",
                                old, new_basic_type
                            ),
                            (None, Some(new)) => format!(
                                "Set validator to {:?}. Type is now {:?}.",
                                new, new_basic_type
                            ),
                            (None, None) => format!(
                                "Validator remains None. Type is {:?}.",
                                new_basic_type
                            ),
                        };
                        let full_msg = format!(
                            "Updated validator for col {} ('{}') of sheet '{:?}/{}'. {}",
                            col_index + 1, // User-facing index
                            column_def.header, // Include column header
                            category,
                            sheet_name,
                            change_msg
                        );
                        info!("{}", full_msg);
                        if old_type != new_basic_type {
                            info!(
                                "Column basic data type changed from {:?} to {:?}.",
                                old_type, new_basic_type
                            );
                        }
                        feedback_writer.write(SheetOperationFeedback {
                            message: full_msg,
                            is_error: false,
                        });

                        metadata_cache = Some(metadata.clone());
                    } else {
                         error!("Metadata missing during mutable access for sheet '{:?}/{}'.", category, sheet_name);
                    }
                } else {
                     error!("Sheet '{:?}/{}' not found during mutable access.", category, sheet_name);
                }

                if let Some(meta) = metadata_cache {
                    let key = (category.clone(), sheet_name.clone());
                    sheets_to_save.insert(key, meta);
                }
            }
            Err(err_msg) => {
                let msg = format!(
                    "Failed validator update for col {} of sheet '{:?}/{}': {}",
                    col_index + 1, // User-facing index
                    category,
                    sheet_name,
                    err_msg
                );
                error!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        }
    } // End event loop

    // --- Phase 3: Saving ---
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!(
                "Validator updated for '{:?}/{}', triggering save.",
                cat, name
            );
            save_single_sheet(registry_immut, &metadata);
        }
    }
}