// src/ui/elements/popups/column_options_popup.rs
use bevy::prelude::*;
use bevy_egui::egui;
use crate::{sheets::{
    definitions::{ColumnDataType, ColumnValidator, SheetMetadata},
    events::{RequestUpdateColumnName, RequestUpdateColumnValidator},
    resources::SheetRegistry,
}, ui::elements::editor::state::ValidatorTypeChoice};
use crate::ui::elements::editor::state::EditorWindowState;

// Import helpers from the new modules
use super::column_options_ui::{show_column_options_window_ui, ColumnOptionsUiResult};
use super::column_options_validator::apply_validator_update;
use super::column_options_on_close::handle_on_close;

/// Main orchestrator function for the column options popup.
/// Handles initialization, calls UI, applies changes, and manages closing.
pub fn show_column_options_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    column_rename_writer: &mut EventWriter<RequestUpdateColumnName>,
    column_validator_writer: &mut EventWriter<RequestUpdateColumnValidator>,
    registry: &mut SheetRegistry, // Needs ResMut for filter update
) {
    if !state.show_column_options_popup {
        return;
    }

    // --- Initialize popup state fields ---
    if state.column_options_popup_needs_init {
        initialize_popup_state(state, registry); // Use helper function
        state.column_options_popup_needs_init = false;
    }
    // --- End Initialization ---


    // --- Show UI and Get Interaction Results ---
    let ui_result = {
        let registry_immut = &*registry; // Create immutable borrow for UI
        show_column_options_window_ui(ctx, state, registry_immut)
    };


    // --- Handle Apply Logic ---
    let mut needs_manual_save = false; // Track if *only* filter changed
    let mut actions_ok = true; // Track if all actions succeed

    if ui_result.apply_clicked {
        let category = &state.options_column_target_category;
        let sheet_name = &state.options_column_target_sheet;
        let col_index = state.options_column_target_index;
        let mut rename_changed = false;
        let mut filter_changed = false;
        let mut validator_changed = false; // Check validator change later

        // --- 1. Apply Rename ---
        let current_name_opt = registry.get_sheet(category, sheet_name).and_then(|s| s.metadata.as_ref()).and_then(|m| m.column_headers.get(col_index)).cloned();
        let new_name_trimmed = state.options_column_rename_input.trim();
        rename_changed = Some(new_name_trimmed) != current_name_opt.as_deref();

        if rename_changed {
             if new_name_trimmed.is_empty() {
                 warn!("Column rename failed: New name empty."); actions_ok = false;
             } else {
                 // Check for duplicates within the *same* sheet
                 let is_duplicate = registry.get_sheet(category, sheet_name).and_then(|s| s.metadata.as_ref()).map_or(false, |m| m.column_headers.iter().enumerate().any(|(i, h)| i != col_index && h.eq_ignore_ascii_case(new_name_trimmed)));
                 if !is_duplicate {
                      column_rename_writer.send(RequestUpdateColumnName {
                           category: category.clone(),
                           sheet_name: sheet_name.clone(),
                           column_index: col_index,
                           new_name: new_name_trimmed.to_string()
                      });
                 } else {
                      warn!("Column rename failed: Name '{}' duplicates existing.", new_name_trimmed); actions_ok = false;
                 }
             }
        }

        // --- 2. Apply Filter (Directly modify registry) ---
        if actions_ok {
            if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
                 if let Some(meta) = &mut sheet_data.metadata {
                     if col_index < meta.column_filters.len() {
                         let filter_to_store: Option<String> = if state.options_column_filter_input.trim().is_empty() { None } else { Some(state.options_column_filter_input.trim().to_string()) };
                         if meta.column_filters[col_index] != filter_to_store {
                             meta.column_filters[col_index] = filter_to_store;
                             filter_changed = true; // Mark filter as changed
                             info!("Filter updated directly for col {} of '{:?}/{}'. Marking potentially for save.", col_index + 1, category, sheet_name);
                         }
                     } else { warn!("Filter update failed: Index out of bounds."); actions_ok = false; }
                 } else { warn!("Filter update failed: Metadata missing."); actions_ok = false; }
             } else { warn!("Filter update failed: Sheet not found."); actions_ok = false; }
        }

        // --- 3. Apply Validator Update (using helper) ---
        if actions_ok {
            let registry_immut = &*registry; // Immutable borrow for validator check/apply
            // Need to check if validator *actually* changed before sending event
            let current_validator = registry_immut.get_sheet(category, sheet_name)
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.column_validators.get(col_index))
                .cloned()
                .flatten();
            let (new_validator_opt, validation_ok) = match state.options_validator_type {
                Some(ValidatorTypeChoice::Basic) => (Some(ColumnValidator::Basic(state.options_basic_type_select)), true),
                Some(ValidatorTypeChoice::Linked) => {
                    if let (Some(ts), Some(tc)) = (state.options_link_target_sheet.as_ref(), state.options_link_target_column_index) {
                        (Some(ColumnValidator::Linked { target_sheet_name: ts.clone(), target_column_index: tc }), true)
                    } else { (None, false) }
                },
                None => (None, false)
            };

            if !validation_ok { actions_ok = false; }
            else {
                validator_changed = current_validator != new_validator_opt;
                if validator_changed {
                    if !apply_validator_update(state, registry_immut, column_validator_writer) {
                         actions_ok = false; // apply_validator_update returns false on internal error
                    }
                }
            }
        }

        // Determine if manual save needed (only filter changed)
        needs_manual_save = filter_changed && !rename_changed && !validator_changed;

    } // End apply_clicked block


    // --- Handle Closing ---
    let should_close = (ui_result.apply_clicked && actions_ok) || ui_result.cancel_clicked || ui_result.close_via_x;

    if should_close {
        // Call the on_close handler
        let registry_immut = &*registry; // Immutable borrow for save handler
        handle_on_close(state, registry_immut, needs_manual_save);
    }
}


/// Helper function to initialize the popup state based on current sheet data.
fn initialize_popup_state(state: &mut EditorWindowState, registry: &SheetRegistry) {
    let target_category = &state.options_column_target_category;
    let target_sheet = &state.options_column_target_sheet;
    let col_index = state.options_column_target_index;

    if let Some(sheet_data) = registry.get_sheet(target_category, target_sheet) {
        if let Some(meta) = &sheet_data.metadata {
            if col_index < meta.column_headers.len() {
                state.options_column_rename_input = meta.column_headers[col_index].clone();
                state.options_column_filter_input = meta.column_filters.get(col_index).cloned().flatten().unwrap_or_default();
                if let Some(Some(validator)) = meta.column_validators.get(col_index) {
                    match validator {
                        ColumnValidator::Basic(data_type) => {
                            state.options_validator_type = Some(ValidatorTypeChoice::Basic);
                            state.options_basic_type_select = *data_type;
                            state.options_link_target_sheet = None;
                            state.options_link_target_column_index = None;
                        }
                        ColumnValidator::Linked { target_sheet_name, target_column_index } => {
                            state.options_validator_type = Some(ValidatorTypeChoice::Linked);
                            state.options_link_target_sheet = Some(target_sheet_name.clone());
                            state.options_link_target_column_index = Some(*target_column_index);
                            // Use actual type from metadata for basic type selection consistency
                            state.options_basic_type_select = meta.column_types.get(col_index).copied().unwrap_or_default();
                        }
                    }
                } else {
                    // Default init if validator missing for the column
                    state.options_validator_type = Some(ValidatorTypeChoice::Basic);
                    state.options_basic_type_select = meta.column_types.get(col_index).copied().unwrap_or_default();
                    state.options_link_target_sheet = None;
                    state.options_link_target_column_index = None;
                 }
            } else { /* Handle index error */ state.options_validator_type = None; }
        } else { /* Handle metadata missing */ state.options_validator_type = None; }
    } else { /* Handle sheet missing */ state.options_validator_type = None; }
}