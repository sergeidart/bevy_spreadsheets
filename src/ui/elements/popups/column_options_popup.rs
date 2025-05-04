// src/ui/elements/popups/column_options_popup.rs
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::HashSet;

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    events::{RequestUpdateColumnName, RequestUpdateColumnValidator},
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::{EditorWindowState, ValidatorTypeChoice};


pub fn show_column_options_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    column_rename_writer: &mut EventWriter<RequestUpdateColumnName>,
    column_validator_writer: &mut EventWriter<RequestUpdateColumnValidator>,
    registry: &mut SheetRegistry,
) {
    if !state.show_column_options_popup {
        return;
    }

    // --- Initialize popup state fields ---
    if state.column_options_popup_needs_init {
        state.column_options_popup_needs_init = false;
        let registry_immut = &*registry; // Use immutable borrow
        if let Some(sheet_data) = registry_immut.get_sheet(&state.options_column_target_sheet) {
            if let Some(meta) = &sheet_data.metadata {
                let col_index = state.options_column_target_index;
                if col_index < meta.column_headers.len() {
                    state.options_column_rename_input = meta.column_headers[col_index].clone();
                    state.options_column_filter_input = meta.column_filters.get(col_index).cloned().flatten().unwrap_or_default();
                    if let Some(Some(validator)) = meta.column_validators.get(col_index) {
                        match validator {
                            ColumnValidator::Basic(data_type) => {
                                state.options_validator_type = Some(ValidatorTypeChoice::Basic);
                                state.options_basic_type_select = *data_type; // Copy is fine
                                state.options_link_target_sheet = None;
                                state.options_link_target_column_index = None;
                            }
                            ColumnValidator::Linked { target_sheet_name, target_column_index } => {
                                state.options_validator_type = Some(ValidatorTypeChoice::Linked);
                                state.options_link_target_sheet = Some(target_sheet_name.clone());
                                state.options_link_target_column_index = Some(*target_column_index); // Copy is fine
                                state.options_basic_type_select = meta.column_types.get(col_index).copied().unwrap_or_default();
                            }
                        }
                    } else { /* Default init if validator missing */ state.options_validator_type = Some(ValidatorTypeChoice::Basic); state.options_basic_type_select = meta.column_types.get(col_index).copied().unwrap_or_default(); state.options_link_target_sheet = None; state.options_link_target_column_index = None; }
                } else { /* Handle index error */ state.options_validator_type = None; }
            } else { /* Handle metadata missing */ state.options_validator_type = None; }
        } else { /* Handle sheet missing */ state.options_validator_type = None; }
    }
    // --- End Initialization ---

    let mut popup_open = state.show_column_options_popup;
    let mut cancel_clicked = false;
    let mut apply_clicked = false;

    egui::Window::new("Column Options")
        .collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut popup_open)
        .show(ctx, |ui| {
            let registry_immut_ui = &*registry; // Use immutable borrow
            let header_text = registry_immut_ui.get_sheet(&state.options_column_target_sheet).and_then(|s| s.metadata.as_ref()).and_then(|m| m.column_headers.get(state.options_column_target_index)).map(|s| s.as_str()).unwrap_or("?");
            ui.label(format!("Options for column '{}' (#{})", header_text, state.options_column_target_index + 1));
            ui.separator();

            // Rename Section
            ui.strong("Rename");
            ui.horizontal(|ui| { ui.label("New Name:"); if ui.add(egui::TextEdit::singleline(&mut state.options_column_rename_input).desired_width(150.0).lock_focus(true)).lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { if !state.options_column_rename_input.trim().is_empty() { apply_clicked = true; } } });

            ui.separator();

            // Filter Section
            ui.strong("Filter (Contains)");
            ui.horizontal(|ui| { ui.label("Text:"); if ui.add(egui::TextEdit::singleline(&mut state.options_column_filter_input).desired_width(150.0)).lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { apply_clicked = true; } if ui.button("Clear").clicked() { state.options_column_filter_input.clear(); } });
            ui.small("Leave empty or clear to disable filter.");

            // Validator Section
            ui.separator();
            ui.strong("Validation Rule");
            if let Some(mut choice) = state.options_validator_type {
                 ui.horizontal(|ui| { ui.radio_value(&mut choice, ValidatorTypeChoice::Basic, "Basic Type"); ui.radio_value(&mut choice, ValidatorTypeChoice::Linked, "Linked Column"); });
                 state.options_validator_type = Some(choice);
                 match choice {
                    ValidatorTypeChoice::Basic => { /* Basic Type Selector UI */
                        ui.horizontal(|ui| {
                             ui.label("Data Type:");
                             egui::ComboBox::from_id_source("basic_type_selector").selected_text(format!("{:?}", state.options_basic_type_select)).show_ui(ui, |ui| { use ColumnDataType::*; let all_types = [ String, OptionString, Bool, OptionBool, U8, OptionU8, U16, OptionU16, U32, OptionU32, U64, OptionU64, I8, OptionI8, I16, OptionI16, I32, OptionI32, I64, OptionI64, F32, OptionF32, F64, OptionF64 ]; for t in all_types.iter() { ui.selectable_value(&mut state.options_basic_type_select, *t, format!("{:?}", t)); } });
                        });
                    }
                    ValidatorTypeChoice::Linked => { /* Linked Column Selector UI */
                        ui.horizontal(|ui| {
                             ui.label("Target Sheet:");
                             let sheets = registry_immut_ui.get_sheet_names().clone(); let selected_sheet_text = state.options_link_target_sheet.as_deref().unwrap_or("--Select--"); let sheet_combo_resp = egui::ComboBox::from_id_source("link_sheet_selector").selected_text(selected_sheet_text).show_ui(ui, |ui| { ui.selectable_value(&mut state.options_link_target_sheet, None, "--Select--"); for name in sheets.iter() { ui.selectable_value(&mut state.options_link_target_sheet, Some(name.clone()), name); } }); if sheet_combo_resp.response.changed() { state.options_link_target_column_index = None; }
                        });
                         ui.horizontal(|ui| {
                             ui.label("Target Column:");
                             ui.add_enabled_ui(state.options_link_target_sheet.is_some(), |ui| {
                                 let mut headers: Vec<(usize, String)> = Vec::new(); if let Some(tsn) = &state.options_link_target_sheet { if let Some(ts) = registry_immut_ui.get_sheet(tsn) { if let Some(m) = &ts.metadata { headers = m.column_headers.iter().cloned().enumerate().collect(); } } } let selected_col_text = match state.options_link_target_column_index { Some(idx) => headers.iter().find(|(i,_)|*i==idx).map(|(_,h)|h.as_str()).unwrap_or("--Invalid--"), None => "--Select--", }; egui::ComboBox::from_id_source("link_column_selector").selected_text(selected_col_text).show_ui(ui, |ui| { ui.selectable_value(&mut state.options_link_target_column_index, None, "--Select--"); for (idx, header_name) in headers.iter() { ui.selectable_value(&mut state.options_link_target_column_index, Some(*idx), header_name); } });
                             });
                         });
                    }
                 }
            } else { ui.colored_label(egui::Color32::RED, "Error."); }

            // Action Buttons
            ui.separator();
            ui.horizontal(|ui| { let apply_enabled = !state.options_column_rename_input.trim().is_empty() && match state.options_validator_type { Some(ValidatorTypeChoice::Linked)=>state.options_link_target_sheet.is_some() && state.options_link_target_column_index.is_some(), _=>true, }; if ui.add_enabled(apply_enabled, egui::Button::new("Apply")).clicked() { apply_clicked = true; } if ui.button("Cancel").clicked() { cancel_clicked = true; } });
        }); // End .show()

    // --- Logic AFTER the window UI ---
    let mut close_popup = false;
    if apply_clicked {
        let sheet_name = state.options_column_target_sheet.clone();
        let col_index = state.options_column_target_index;
        let mut actions_ok = true;

        // Apply Rename
        let current_name_opt = registry.get_sheet(&sheet_name).and_then(|s| s.metadata.as_ref()).and_then(|m| m.column_headers.get(col_index)).cloned();
        let new_name_trimmed = state.options_column_rename_input.trim();
        if Some(new_name_trimmed) != current_name_opt.as_deref() {
             if new_name_trimmed.is_empty() { actions_ok = false; }
             else {
                 let is_duplicate = registry.get_sheet(&sheet_name).and_then(|s| s.metadata.as_ref()).map_or(false, |m| m.column_headers.iter().enumerate().any(|(i, h)| i != col_index && h.eq_ignore_ascii_case(new_name_trimmed)));
                 if !is_duplicate { column_rename_writer.send(RequestUpdateColumnName { sheet_name: sheet_name.clone(), column_index: col_index, new_name: new_name_trimmed.to_string() }); }
                 else { actions_ok = false; }
             }
        }

        // Apply Filter
        let mut filter_changed = false;
        if actions_ok {
             if let Some(sheet_data) = registry.get_sheet_mut(&sheet_name) {
                  if let Some(meta) = &mut sheet_data.metadata {
                      if col_index < meta.column_filters.len() {
                          let filter_to_store: Option<String> = if state.options_column_filter_input.trim().is_empty() { None } else { Some(state.options_column_filter_input.trim().to_string()) };
                          if meta.column_filters[col_index] != filter_to_store { meta.column_filters[col_index] = filter_to_store; filter_changed = true; }
                      } else { actions_ok = false; }
                  } else { actions_ok = false; }
              } else { actions_ok = false; }
        }

        // Construct New Validator
        let new_validator: Option<ColumnValidator> = match state.options_validator_type {
            Some(ValidatorTypeChoice::Basic) => Some(ColumnValidator::Basic(state.options_basic_type_select)),
            Some(ValidatorTypeChoice::Linked) => { if let (Some(ts), Some(tc)) = (state.options_link_target_sheet.as_ref(), state.options_link_target_column_index) { Some(ColumnValidator::Linked { target_sheet_name: ts.clone(), target_column_index: tc }) } else { actions_ok = false; None } },
            None => { actions_ok = false; None }
        };

        // Send Validator Update Event
        let current_validator = registry.get_sheet(&sheet_name).and_then(|s| s.metadata.as_ref()).and_then(|m| m.column_validators.get(col_index)).cloned().flatten();
        if current_validator != new_validator && actions_ok {
            // --- Clone new_validator when sending ---
            column_validator_writer.send(RequestUpdateColumnValidator {
                 sheet_name: sheet_name.clone(),
                 column_index: col_index,
                 new_validator: new_validator.clone(), // Clone here
            });
            // --- End Clone ---
        }

        // Manual Save Trigger (if needed)
        let rename_did_not_change = Some(new_name_trimmed) == current_name_opt.as_deref();
        // --- Comparison using original new_validator (before potential clone) ---
        let validator_did_not_change = current_validator == new_validator;
        if filter_changed && rename_did_not_change && validator_did_not_change && actions_ok {
             warn!("Filter-only change detected - manual save trigger needs refactor.");
             // TODO: Refactor filter update to use events.
        }

        if actions_ok { close_popup = true; }
    }

    if cancel_clicked { close_popup = true; }
    if !close_popup && !popup_open { close_popup = true; }

    if close_popup {
        state.show_column_options_popup = false;
        state.options_column_target_sheet.clear();
        state.options_column_target_index = 0;
        state.options_column_rename_input.clear();
        state.options_column_filter_input.clear();
        state.column_options_popup_needs_init = false;
        state.options_validator_type = None;
        state.options_link_target_sheet = None;
        state.options_link_target_column_index = None;
    } else {
        state.show_column_options_popup = popup_open;
    }
}