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
    // Detect if column already has Structure validator
    let existing_is_structure = registry_immut
        .get_sheet(&state.options_column_target_category, &state.options_column_target_sheet)
        .and_then(|s| s.metadata.as_ref())
        .and_then(|m| m.columns.get(state.options_column_target_index))
        .map(|c| matches!(c.validator, Some(ColumnValidator::Structure)))
        .unwrap_or(false);

    // NOTE: Key Column UI now shown only when Structure choice is selected (inside match below)

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
            ui.radio_value(
                &mut choice,
                ValidatorTypeChoice::Structure,
                "Structure",
            );
        });
        state.options_validator_type = Some(choice); // Update state

        match choice {
            ValidatorTypeChoice::Basic => {
                /* Basic Type Selector UI */
                ui.horizontal(|ui| {
                    ui.label("Data Type:");
                    egui::ComboBox::from_id_salt("basic_type_selector")
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
                                ui.selectable_value(&mut state.options_basic_type_select, *t, format!("{:?}", t));
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
                        egui::ComboBox::from_id_salt("link_sheet_selector")
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
                            egui::ComboBox::from_id_salt("link_column_selector")
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
            ValidatorTypeChoice::Structure => {
                // Two modes: creating new structure vs editing existing one.
                let meta_opt = registry_immut
                    .get_sheet(&state.options_column_target_category, &state.options_column_target_sheet)
                    .and_then(|s| s.metadata.as_ref());
                if existing_is_structure {
                    ui.colored_label(egui::Color32::LIGHT_BLUE, "Structure established");
                    if let Some(meta) = meta_opt {
                        if let Some(col) = meta.columns.get(state.options_column_target_index) {
                            state.options_existing_structure_key_parent_column = col.structure_key_parent_column_index;
                        }
                        let mut current_key = state.options_existing_structure_key_parent_column;
                        ui.horizontal(|ui_h| {
                            ui_h.label("Key Column:");
                            let (headers, is_parent_headers) = if let Some(parent_link) = &meta.structure_parent {
                                if let Some(parent_sheet) = registry_immut.get_sheet(&parent_link.parent_category, &parent_link.parent_sheet) {
                                    if let Some(parent_meta) = &parent_sheet.metadata { (parent_meta.columns.iter().map(|c| c.header.clone()).collect::<Vec<_>>(), true) } else { (Vec::new(), true) }
                                } else { (Vec::new(), true) }
                            } else { (meta.columns.iter().map(|c| c.header.clone()).collect::<Vec<_>>(), false) };
                            if headers.is_empty() { ui_h.label("<no headers>"); return; }
                            let sel_text = current_key.and_then(|i| headers.get(i)).cloned().unwrap_or_else(|| "(none)".to_string());
                            egui::ComboBox::from_id_salt("key_parent_column_selector")
                                .selected_text(sel_text)
                                .show_ui(ui_h, |ui_c| {
                                    ui_c.selectable_value(&mut current_key, None, "(none)");
                                    for (i, h) in headers.iter().enumerate() { if !is_parent_headers && i == state.options_column_target_index { continue; } ui_c.selectable_value(&mut current_key, Some(i), h); }
                                });
                            if current_key.is_some() { if ui_h.small_button("x").on_hover_text("Clear key").clicked() { current_key = None; } }
                            if current_key != state.options_existing_structure_key_parent_column {
                                state.options_existing_structure_key_parent_column = current_key;
                                state.pending_structure_key_apply = Some((state.options_column_target_category.clone(), state.options_column_target_sheet.clone(), state.options_column_target_index, current_key));
                            }
                        });
                        ui.label("Key column is context-only (sent first to AI) and not overwritten.");
                    }
                } else {
                    // --- Key Column selection (first) ---
                    if let Some(meta) = meta_opt {
                        // If we are creating the structure from the parent sheet (usual case) there is no structure_parent yet.
                        // Use current sheet headers (excluding the target structure column itself) as potential key columns.
                        let (headers, is_parent_headers) = if let Some(parent_link) = &meta.structure_parent {
                            if let Some(parent_sheet) = registry_immut.get_sheet(&parent_link.parent_category, &parent_link.parent_sheet) {
                                if let Some(parent_meta) = &parent_sheet.metadata { (parent_meta.columns.iter().map(|c| c.header.clone()).collect::<Vec<_>>(), true) } else { (Vec::new(), true) }
                            } else { (Vec::new(), true) }
                        } else {
                            (meta.columns.iter().map(|c| c.header.clone()).collect::<Vec<_>>(), false)
                        };
                        ui.horizontal(|ui_k| {
                            ui_k.label("Key Column:");
                            if headers.is_empty() { ui_k.label("<none>"); return; }
                            let mut temp_choice = state.options_structure_key_parent_column_temp;
                            let sel_text = temp_choice.and_then(|i| headers.get(i)).cloned().unwrap_or_else(|| "(none)".to_string());
                            egui::ComboBox::from_id_salt("new_structure_key_parent_col")
                                .selected_text(sel_text)
                                .show_ui(ui_k, |ui_c| {
                                    ui_c.selectable_value(&mut temp_choice, None, "(none)");
                                    for (i,h) in headers.iter().enumerate() {
                                        // When using current sheet headers, exclude the structure column itself.
                                        if !is_parent_headers && i == state.options_column_target_index { continue; }
                                        ui_c.selectable_value(&mut temp_choice, Some(i), h);
                                    }
                                });
                            if temp_choice.is_some() { if ui_k.small_button("x").on_hover_text("Clear key selection").clicked() { temp_choice = None; } }
                            if temp_choice != state.options_structure_key_parent_column_temp { state.options_structure_key_parent_column_temp = temp_choice; }
                            ui_k.label("(Optional context only, not overwritten)");
                        });
                    }
                    ui.add(egui::Separator::default());
                    ui.label("Schema: choose source columns to copy into object fields.");
                    let (headers, self_index) = meta_opt.map(|m| (m.columns.iter().map(|c| c.header.clone()).collect::<Vec<_>>(), state.options_column_target_index)).unwrap_or_default();
                    if state.options_structure_source_columns.is_empty() { state.options_structure_source_columns.push(None); }
                    let mut to_remove: Vec<usize> = Vec::new();
                    for i in 0..state.options_structure_source_columns.len() { ui.horizontal(|ui_h| { let mut val = state.options_structure_source_columns[i]; egui::ComboBox::from_id_salt(format!("structure_src_col_{}", i)).selected_text(match val { Some(idx) => headers.get(idx).cloned().unwrap_or_else(|| "Invalid".to_string()), None => "None".to_string() }).show_ui(ui_h, |ui_c| { ui_c.selectable_value(&mut val, None, "None"); for (idx, header) in headers.iter().enumerate() { if idx == self_index { continue; } ui_c.selectable_value(&mut val, Some(idx), header); } }); if val != state.options_structure_source_columns[i] { state.options_structure_source_columns[i] = val; } if i + 1 < state.options_structure_source_columns.len() && ui_h.button("X").clicked() { to_remove.push(i); } }); }
                    if !to_remove.is_empty() { for idx in to_remove.into_iter().rev() { if idx < state.options_structure_source_columns.len() { state.options_structure_source_columns.remove(idx); } } }
                    let need_new = state.options_structure_source_columns.last().map(|v| v.is_some()).unwrap_or(false); if need_new { state.options_structure_source_columns.push(None); }
                }
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
            col_index + 1, category, sheet_name
        );
        column_validator_writer.write(RequestUpdateColumnValidator {
            category: category.clone(), // <<< Send category
            sheet_name: sheet_name.clone(),
            column_index: col_index,
            new_validator: new_validator.clone(),
            structure_source_columns: if matches!(new_validator, Some(ColumnValidator::Structure)) {
                let sources: Vec<usize> = state.options_structure_source_columns.iter().filter_map(|o| *o).collect();
                if sources.is_empty() { None } else { Some(sources) }
            } else { None },
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
        Some(ValidatorTypeChoice::Structure) => true, // Always valid; confirmation handles risk
        Some(ValidatorTypeChoice::Basic) => true, // Always valid
        None => false,
    }
}