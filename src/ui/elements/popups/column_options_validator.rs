// src/ui/elements/popups/column_options_validator.rs
use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    events::RequestUpdateColumnValidator,
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::{
    EditorWindowState, ValidatorTypeChoice,
};
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::HashSet; // Keep HashSet for potential future use

/// Renders the UI section for selecting the column validator rule.
pub(super) fn show_validator_section(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry_immut: &SheetRegistry,
) {
    ui.strong("Validation Rule");
    if let Some(mut choice) = state.options_validator_type {
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut choice,
                ValidatorTypeChoice::Basic,
                "Basic Type",
            );
            ui.radio_value(
                &mut choice,
                ValidatorTypeChoice::Linked,
                "Linked Column",
            );
        });
        state.options_validator_type = Some(choice); // Update state

        match choice {
            ValidatorTypeChoice::Basic => {
                /* Basic Type Selector UI */
                ui.horizontal(|ui| {
                    ui.label("Data Type:");
                    egui::ComboBox::from_id_source("basic_type_selector")
                        .selected_text(format!(
                            "{:?}",
                            state.options_basic_type_select
                        ))
                        .show_ui(ui, |ui| {
                            use ColumnDataType::*;
                            let all_types = [
                                String,
                                OptionString,
                                Bool,
                                OptionBool,
                                U8,
                                OptionU8,
                                U16,
                                OptionU16,
                                U32,
                                OptionU32,
                                U64,
                                OptionU64,
                                I8,
                                OptionI8,
                                I16,
                                OptionI16,
                                I32,
                                OptionI32,
                                I64,
                                OptionI64,
                                F32,
                                OptionF32,
                                F64,
                                OptionF64,
                            ];
                            for t in all_types.iter() {
                                ui.selectable_value(
                                    &mut state.options_basic_type_select,
                                    *t,
                                    format!("{:?}", t),
                                );
                            }
                        });
                });
            }
            ValidatorTypeChoice::Linked => {
                /* Linked Column Selector UI */
                ui.horizontal(|ui| {
                    ui.label("Target Sheet:");
                    // Get all sheet names across all categories for linking
                    let mut all_sheet_names: Vec<String> = registry_immut
                        .iter_sheets()
                        .map(|(_, name, _)| name.clone())
                        .collect::<HashSet<_>>() // Deduplicate
                        .into_iter()
                        .collect();
                    all_sheet_names.sort_unstable(); // Sort names

                    let selected_sheet_text = state
                        .options_link_target_sheet
                        .as_deref()
                        .unwrap_or("--Select--");
                    let sheet_combo_resp =
                        egui::ComboBox::from_id_source("link_sheet_selector")
                            .selected_text(selected_sheet_text)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.options_link_target_sheet,
                                    None,
                                    "--Select--",
                                );
                                for name in all_sheet_names.iter() {
                                    ui.selectable_value(
                                        &mut state.options_link_target_sheet,
                                        Some(name.clone()),
                                        name,
                                    );
                                }
                            });
                    // If sheet selection changes, clear column selection
                    if sheet_combo_resp.response.changed() {
                        state.options_link_target_column_index = None;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Target Column:");
                    ui.add_enabled_ui(
                        state.options_link_target_sheet.is_some(),
                        |ui| {
                            let mut headers: Vec<(usize, String)> = Vec::new();
                            if let Some(tsn) = &state.options_link_target_sheet
                            {
                                // Find the target sheet (any category) and get its headers
                                if let Some((_, _, ts_data)) = registry_immut
                                    .iter_sheets()
                                    .find(|(_, name, _)| {
                                        name.as_str() == tsn.as_str()
                                    })
                                {
                                    if let Some(m) = &ts_data.metadata {
                                        // --- CORRECTED: Get headers from columns ---
                                        headers = m
                                            .columns
                                            .iter()
                                            .map(|c| c.header.clone())
                                            .enumerate() // Get index along with header
                                            .collect();
                                    }
                                }
                            }
                            let selected_col_text =
                                match state.options_link_target_column_index {
                                    Some(idx) => headers
                                        .iter()
                                        .find(|(i, _)| *i == idx)
                                        .map(|(_, h)| h.as_str())
                                        .unwrap_or("--Invalid--"),
                                    None => "--Select--",
                                };
                            egui::ComboBox::from_id_source(
                                "link_column_selector",
                            )
                            .selected_text(selected_col_text)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.options_link_target_column_index,
                                    None,
                                    "--Select--",
                                );
                                for (idx, header_name) in headers.iter() {
                                    ui.selectable_value(
                                        &mut state.options_link_target_column_index,
                                        Some(*idx),
                                        header_name,
                                    );
                                }
                            });
                        },
                    );
                });
            }
        }
    } else {
        ui.colored_label(
            egui::Color32::RED,
            "Error loading validator options.",
        );
    }
}

/// Checks the current state and sends a validator update event if needed.
/// Returns true if the action was successful (or no change needed), false if validation failed.
pub(super) fn apply_validator_update(
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
            col_index + 1, category, sheet_name
        );
        column_validator_writer.write(RequestUpdateColumnValidator {
            category: category.clone(), // <<< Send category
            sheet_name: sheet_name.clone(),
            column_index: col_index,
            new_validator: new_validator.clone(),
        });
    } else {
        trace!(
            "Validator unchanged for col {} of '{:?}/{}'.",
            col_index + 1, category, sheet_name
        );
    }

    true // Action successful or no change needed
}

/// Checks if the current validator configuration in the state is valid for applying.
pub(super) fn is_validator_config_valid(state: &EditorWindowState) -> bool {
    match state.options_validator_type {
        Some(ValidatorTypeChoice::Linked) => {
            state.options_link_target_sheet.is_some()
                && state.options_link_target_column_index.is_some()
        }
        Some(ValidatorTypeChoice::Basic) => true, // Basic is always valid here
        None => false, // Invalid state if options didn't load
    }
}