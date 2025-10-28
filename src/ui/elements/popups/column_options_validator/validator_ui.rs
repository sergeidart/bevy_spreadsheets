// src/ui/elements/popups/column_options_validator/validator_ui.rs
// UI rendering for column validator options

use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator},
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::{EditorWindowState, ValidatorTypeChoice};
use bevy_egui::egui;
use std::collections::HashSet;

// Import helper modules
use super::filter_widgets::{render_filter_box, show_filtered_popup_selector};
use super::schema_helpers::{
    get_all_headers, get_existing_structure_key_info, get_headers_with_indices,
    get_new_structure_headers,
};
use super::state_sync::{
    sync_existing_structure_key_state, sync_new_structure_key_temp_state,
    update_pending_structure_key_apply,
};

/// Renders the UI section for selecting the column validator rule.
pub fn show_validator_section(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry_immut: &SheetRegistry,
) {
    ui.strong("Validation Rule");
    // Detect if column already has Structure validator
    let existing_is_structure = registry_immut
        .get_sheet(
            &state.options_column_target_category,
            &state.options_column_target_sheet,
        )
        .and_then(|s| s.metadata.as_ref())
        .and_then(|m| m.columns.get(state.options_column_target_index))
        .map(|c| matches!(c.validator, Some(ColumnValidator::Structure)))
        .unwrap_or(false);

    // NOTE: Key Column UI now shown only when Structure choice is selected (inside match below)

    if let Some(mut choice) = state.options_validator_type {
        ui.horizontal(|ui| {
            ui.radio_value(&mut choice, ValidatorTypeChoice::Basic, "Basic Type");
            ui.radio_value(&mut choice, ValidatorTypeChoice::Linked, "Linked Column");
            ui.radio_value(&mut choice, ValidatorTypeChoice::Structure, "Structure");
        });
        state.options_validator_type = Some(choice); // Update state

        match choice {
            ValidatorTypeChoice::Basic => {
                show_basic_type_selector(ui, state);
            }
            ValidatorTypeChoice::Linked => {
                show_linked_column_selectors(ui, state, registry_immut);
            }
            ValidatorTypeChoice::Structure => {
                show_structure_validator_ui(ui, state, registry_immut, existing_is_structure);
            }
        }
    } else {
        ui.colored_label(egui::Color32::RED, "Error loading validator options.");
    }
}

/// Renders the basic type selector dropdown
fn show_basic_type_selector(ui: &mut egui::Ui, state: &mut EditorWindowState) {
    ui.horizontal(|ui| {
        ui.label("Data Type:");
        egui::ComboBox::from_id_salt("basic_type_selector")
            .selected_text(format!("{:?}", state.options_basic_type_select))
            .show_ui(ui, |ui| {
                use ColumnDataType::*;
                let all_types = [String, Bool, I64, F64];
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

/// Renders the linked column target sheet and column selectors
fn show_linked_column_selectors(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry_immut: &SheetRegistry,
) {
    // Target Sheet (internal filter inside popup like top panel style)
    ui.horizontal(|ui| {
        ui.label("Target Sheet:");
        let mut all_sheet_names: Vec<String> = registry_immut
            .iter_sheets()
            .map(|(_, name, _)| name.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        all_sheet_names.sort_unstable();
        let prev_sheet = state.options_link_target_sheet.clone();
        let display = state
            .options_link_target_sheet
            .as_deref()
            .unwrap_or("--Select--")
            .to_string();
        let combo_id = "link_sheet_selector_internal".to_string();
        
        show_filtered_popup_selector(
            ui,
            &combo_id,
            &display,
            &all_sheet_names,
            &mut state.options_link_target_sheet,
            None,
            "type to filter sheets",
        );
        
        if state.options_link_target_sheet != prev_sheet {
            state.options_link_target_column_index = None;
        }
    });

    // Target Column (internal filter popup)
    ui.horizontal(|ui| {
        ui.label("Target Column:");
        ui.add_enabled_ui(state.options_link_target_sheet.is_some(), |ui| {
            let mut headers: Vec<(usize, String)> = Vec::new();
            if let Some(tsn) = &state.options_link_target_sheet {
                if let Some((_, _, ts_data)) = registry_immut
                    .iter_sheets()
                    .find(|(_, name, _)| name.as_str() == tsn.as_str())
                {
                    if let Some(m) = &ts_data.metadata {
                        headers = get_headers_with_indices(m);
                    }
                }
            }
            let selected_col_text = match state.options_link_target_column_index {
                Some(idx) => headers
                    .iter()
                    .find(|(i, _)| *i == idx)
                    .map(|(_, h)| h.as_str())
                    .unwrap_or("--Invalid--"),
                None => "--Select--",
            };
            let display = selected_col_text.to_string();
            let combo_id = format!(
                "link_column_selector_internal_{}_{}_{}",
                state
                    .options_column_target_category
                    .as_deref()
                    .unwrap_or(""),
                state.options_link_target_sheet.as_deref().unwrap_or(""),
                state.options_column_target_index
            );
            
            let header_strings: Vec<String> = headers.iter().map(|(_, h)| h.clone()).collect();
            let mut selected_string = state
                .options_link_target_column_index
                .and_then(|idx| headers.iter().find(|(i, _)| *i == idx))
                .map(|(_, h)| h.clone());
            
            show_filtered_popup_selector(
                ui,
                &combo_id,
                &display,
                &header_strings,
                &mut selected_string,
                None,
                "type to filter columns",
            );
            
            // Update index if selection changed
            if let Some(ref selected) = selected_string {
                state.options_link_target_column_index = headers
                    .iter()
                    .find(|(_, h)| h == selected)
                    .map(|(i, _)| *i);
            } else if state.options_link_target_column_index.is_some() && selected_string.is_none() {
                state.options_link_target_column_index = None;
            }
        });
    });
}

/// Renders the structure validator UI (either editing existing or creating new)
fn show_structure_validator_ui(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry_immut: &SheetRegistry,
    existing_is_structure: bool,
) {
    let meta_opt = registry_immut
        .get_sheet(
            &state.options_column_target_category,
            &state.options_column_target_sheet,
        )
        .and_then(|s| s.metadata.as_ref());

    if existing_is_structure {
        show_existing_structure_ui(ui, state, meta_opt, registry_immut);
    } else {
        show_new_structure_ui(ui, state, meta_opt);
    }
}

/// Renders UI for editing an existing structure validator
fn show_existing_structure_ui(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    meta_opt: Option<&crate::sheets::definitions::SheetMetadata>,
    registry_immut: &SheetRegistry,
) {
    ui.colored_label(egui::Color32::LIGHT_BLUE, "Structure established");
    if let Some(meta) = meta_opt {
        // Get authoritative headers and current key index using helper
        let (headers, current_key_effective, exclude_idx_opt) =
            get_existing_structure_key_info(state, meta, registry_immut);

        // Sync state only if differs
        sync_existing_structure_key_state(state, current_key_effective);
        let mut current_key = state.options_existing_structure_key_parent_column;
        
        show_key_column_selector(
            ui,
            state,
            &headers,
            &mut current_key,
            exclude_idx_opt,
            "key_parent_column_selector_internal",
        );
        
        if current_key != state.options_existing_structure_key_parent_column {
            state.options_existing_structure_key_parent_column = current_key;
            update_pending_structure_key_apply(state, current_key);
        }
        
        ui.label("Key column is context-only (sent first to AI) and not overwritten.");
    }
}

/// Renders UI for creating a new structure validator
fn show_new_structure_ui(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    meta_opt: Option<&crate::sheets::definitions::SheetMetadata>,
) {
    // --- Key Column selection (first) ---
    if let Some(meta) = meta_opt {
        let headers = get_all_headers(meta);
        let mut temp_choice = state.options_structure_key_parent_column_temp;
        
        show_key_column_selector(
            ui,
            state,
            &headers,
            &mut temp_choice,
            Some(state.options_column_target_index),
            "new_structure_key_parent_col_internal",
        );
        
        sync_new_structure_key_temp_state(state, temp_choice);
    }
    
    // Schema: choose source columns to copy into object fields
    ui.label("Schema: choose source columns to copy into object fields.");
    let headers = meta_opt
        .map(|m| get_new_structure_headers(m))
        .unwrap_or_default();
    
    show_structure_source_column_list(ui, state, &headers);
}

/// Renders the key column selector with filtered popup
fn show_key_column_selector(
    ui: &mut egui::Ui,
    state: &EditorWindowState,
    headers: &[String],
    current_key: &mut Option<usize>,
    exclude_idx_opt: Option<usize>,
    combo_id_prefix: &str,
) {
    ui.horizontal(|ui_k| {
        ui_k.label("Key Column:");
        if headers.is_empty() {
            ui_k.label("<none>");
            return;
        }
        
        let sel_text = current_key
            .and_then(|i| headers.get(i))
            .cloned()
            .unwrap_or_else(|| "(none)".to_string());
        let combo_id = format!(
            "{}_{}_{}", 
            combo_id_prefix,
            state.options_column_target_category.as_deref().unwrap_or(""),
            state.options_column_target_index
        );
        let filter_key = format!("{}_filter", combo_id);
        let button = ui_k.button(sel_text);
        let popup_id = egui::Id::new(combo_id.clone());
        
        if button.clicked() {
            ui_k.ctx().memory_mut(|mem| mem.open_popup(popup_id));
        }
        
        egui::containers::popup::popup_below_widget(
            ui_k,
            popup_id,
            &button,
            egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
            |popup_ui| {
                let mut filter_text = popup_ui.memory(|mem| {
                    mem.data
                        .get_temp::<String>(filter_key.clone().into())
                        .unwrap_or_default()
                });
                
                render_filter_box(popup_ui, &mut filter_text, &filter_key, headers, "type to filter columns");
                let current_filter = filter_text.to_lowercase();
                
                egui::ScrollArea::vertical().max_height(300.0).show(
                    popup_ui,
                    |list_ui| {
                        if list_ui
                            .selectable_label(current_key.is_none(), "(none)")
                            .clicked()
                        {
                            *current_key = None;
                            list_ui.memory_mut(|mem| mem.close_popup());
                        }
                        for (i, h) in headers.iter().enumerate() {
                            // Exclude the structure column itself
                            if let Some(ex_idx) = exclude_idx_opt {
                                if i == ex_idx {
                                    continue;
                                }
                            }
                            // Skip technical columns: row_index, parent_key
                            let hl = h.to_lowercase();
                            if hl == "row_index" || hl == "parent_key" {
                                continue;
                            }
                            if !current_filter.is_empty()
                                && !h.to_lowercase().contains(&current_filter)
                            {
                                continue;
                            }
                            if list_ui
                                .selectable_label(*current_key == Some(i), h)
                                .clicked()
                            {
                                *current_key = Some(i);
                                list_ui.memory_mut(|mem| mem.close_popup());
                            }
                        }
                    },
                );
            },
        );
        
        if current_key.is_some() {
            if ui_k.small_button("x").on_hover_text("Clear key").clicked() {
                *current_key = None;
            }
        }
        
        if !matches!(combo_id_prefix, "key_parent_column_selector_internal") {
            ui_k.label("(Optional context only, not overwritten)");
        }
    });
}

/// Renders the structure source column list (for new structure creation)
fn show_structure_source_column_list(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    headers: &[String],
) {
    if state.options_structure_source_columns.is_empty() {
        state.options_structure_source_columns.push(None);
    }
    
    let mut to_remove: Vec<usize> = Vec::new();
    for i in 0..state.options_structure_source_columns.len() {
        ui.horizontal(|ui_h| {
            let mut val = state.options_structure_source_columns[i];
            let sel_text = match val {
                Some(idx) => headers
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| "Invalid".to_string()),
                None => "None".to_string(),
            };
            let combo_id = format!(
                "structure_src_col_internal_{}_{}",
                i, state.options_column_target_index
            );
            let filter_key = format!("{}_filter", combo_id);
            let btn = ui_h.button(sel_text);
            let popup_id = egui::Id::new(combo_id.clone());
            
            if btn.clicked() {
                ui_h.ctx().memory_mut(|mem| mem.open_popup(popup_id));
            }
            
            egui::containers::popup::popup_below_widget(
                ui_h,
                popup_id,
                &btn,
                egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                |popup_ui| {
                    let mut filter_text = popup_ui.memory(|mem| {
                        mem.data
                            .get_temp::<String>(filter_key.clone().into())
                            .unwrap_or_default()
                    });
                    
                    render_filter_box(popup_ui, &mut filter_text, &filter_key, headers, "type to filter columns");
                    let current_filter = filter_text.to_lowercase();
                    
                    egui::ScrollArea::vertical().max_height(300.0).show(
                        popup_ui,
                        |list_ui| {
                            if list_ui
                                .selectable_label(val.is_none(), "None")
                                .clicked()
                            {
                                val = None;
                                list_ui.memory_mut(|mem| mem.close_popup());
                            }
                            for (idx, header) in headers.iter().enumerate() {
                                // Skip technical columns: row_index, parent_key
                                let hl = header.to_lowercase();
                                if hl == "row_index" || hl == "parent_key" {
                                    continue;
                                }
                                if !current_filter.is_empty()
                                    && !header.to_lowercase().contains(&current_filter)
                                {
                                    continue;
                                }
                                if list_ui
                                    .selectable_label(val == Some(idx), header)
                                    .clicked()
                                {
                                    val = Some(idx);
                                    list_ui.memory_mut(|mem| mem.close_popup());
                                }
                            }
                        },
                    );
                },
            );
            
            if val != state.options_structure_source_columns[i] {
                state.options_structure_source_columns[i] = val;
            }
            if i + 1 < state.options_structure_source_columns.len()
                && ui_h.button("X").clicked()
            {
                to_remove.push(i);
            }
        });
    }
    
    if !to_remove.is_empty() {
        for idx in to_remove.into_iter().rev() {
            if idx < state.options_structure_source_columns.len() {
                state.options_structure_source_columns.remove(idx);
            }
        }
    }
    
    let need_new = state
        .options_structure_source_columns
        .last()
        .map(|v| v.is_some())
        .unwrap_or(false);
    if need_new {
        state.options_structure_source_columns.push(None);
    }
}
