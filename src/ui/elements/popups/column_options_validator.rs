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
                    let display = state.options_link_target_sheet.as_deref().unwrap_or("--Select--").to_string();
                    let combo_id = "link_sheet_selector_internal".to_string();
                    let filter_key = format!("{}_filter", combo_id);
                    let btn = ui.button(display);
                    let popup_id = egui::Id::new(combo_id.clone());
                    if btn.clicked() { ui.ctx().memory_mut(|mem| mem.open_popup(popup_id)); }
                    egui::containers::popup::popup_below_widget(
                        ui,
                        popup_id,
                        &btn,
                        egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                        |popup_ui| {
                            let mut filter_text = popup_ui.memory(|mem| mem.data.get_temp::<String>(filter_key.clone().into()).unwrap_or_default());
                            let char_w = 8.0_f32;
                            let max_name_len = all_sheet_names.iter().map(|s| s.len()).max().unwrap_or(12);
                            let padding = 24.0_f32;
                            let mut popup_min_width = (max_name_len as f32) * char_w + padding;
                            if popup_min_width < 120.0 { popup_min_width = 120.0; }
                            if popup_min_width > 900.0 { popup_min_width = 900.0; }
                            popup_ui.set_min_width(popup_min_width);
                            popup_ui.horizontal(|ui_h| {
                                ui_h.label("Filter:");
                                let avail = ui_h.available_width();
                                let default_chars = 28usize;
                                let desired = (default_chars as f32) * char_w;
                                let width = desired.min(avail).min(popup_min_width - 40.0);
                                let resp = ui_h.add(egui::TextEdit::singleline(&mut filter_text).desired_width(width).hint_text("type to filter sheets"));
                                if resp.changed() { ui_h.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                                if ui_h.small_button("x").clicked() { filter_text.clear(); ui_h.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                            });
                            let current = filter_text.to_lowercase();
                            egui::ScrollArea::vertical().max_height(300.0).show(popup_ui, |list_ui| {
                                if list_ui.selectable_label(state.options_link_target_sheet.is_none(), "--Select--").clicked() {
                                    state.options_link_target_sheet = None;
                                    list_ui.memory_mut(|mem| mem.close_popup());
                                }
                                for name in all_sheet_names.iter() {
                                    if !current.is_empty() && !name.to_lowercase().contains(&current) { continue; }
                                    if list_ui.selectable_label(state.options_link_target_sheet.as_deref() == Some(name.as_str()), name).clicked() {
                                        state.options_link_target_sheet = Some(name.clone());
                                        list_ui.memory_mut(|mem| mem.close_popup());
                                    }
                                }
                            });
                        },
                    );
                    if state.options_link_target_sheet != prev_sheet { state.options_link_target_column_index = None; }
                });

                // Target Column (internal filter popup)
                ui.horizontal(|ui| {
                    ui.label("Target Column:");
                    ui.add_enabled_ui(state.options_link_target_sheet.is_some(), |ui| {
                        let mut headers: Vec<(usize, String)> = Vec::new();
                        if let Some(tsn) = &state.options_link_target_sheet {
                            if let Some((_, _, ts_data)) = registry_immut.iter_sheets().find(|(_, name, _)| name.as_str() == tsn.as_str()) {
                                if let Some(m) = &ts_data.metadata { headers = m.columns.iter().map(|c| c.header.clone()).enumerate().collect(); }
                            }
                        }
                        let selected_col_text = match state.options_link_target_column_index {
                            Some(idx) => headers.iter().find(|(i, _)| *i == idx).map(|(_, h)| h.as_str()).unwrap_or("--Invalid--"),
                            None => "--Select--",
                        };
                        let display = selected_col_text.to_string();
                        let combo_id = format!("link_column_selector_internal_{}_{}_{}", state.options_column_target_category.as_deref().unwrap_or(""), state.options_link_target_sheet.as_deref().unwrap_or(""), state.options_column_target_index);
                        let filter_key = format!("{}_filter", combo_id);
                        let btn = ui.button(display);
                        let popup_id = egui::Id::new(combo_id.clone());
                        if btn.clicked() { ui.ctx().memory_mut(|mem| mem.open_popup(popup_id)); }
                        egui::containers::popup::popup_below_widget(
                            ui,
                            popup_id,
                            &btn,
                            egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                            |popup_ui| {
                                let mut filter_text = popup_ui.memory(|mem| mem.data.get_temp::<String>(filter_key.clone().into()).unwrap_or_default());
                                let char_w = 8.0_f32;
                                let max_name_len = headers.iter().map(|(_, h)| h.len()).max().unwrap_or(12);
                                let padding = 24.0_f32;
                                let mut popup_min_width = (max_name_len as f32) * char_w + padding;
                                if popup_min_width < 120.0 { popup_min_width = 120.0; }
                                if popup_min_width > 900.0 { popup_min_width = 900.0; }
                                popup_ui.set_min_width(popup_min_width);
                                popup_ui.horizontal(|ui_h| {
                                    ui_h.label("Filter:");
                                    let avail = ui_h.available_width();
                                    let default_chars = 28usize;
                                    let desired = (default_chars as f32) * char_w;
                                    let width = desired.min(avail).min(popup_min_width - 40.0);
                                    let resp = ui_h.add(egui::TextEdit::singleline(&mut filter_text).desired_width(width).hint_text("type to filter columns"));
                                    if resp.changed() { ui_h.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                                    if ui_h.small_button("x").clicked() { filter_text.clear(); ui_h.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                                });
                                let current = filter_text.to_lowercase();
                                egui::ScrollArea::vertical().max_height(300.0).show(popup_ui, |list_ui| {
                                    if list_ui.selectable_label(state.options_link_target_column_index.is_none(), "--Select--").clicked() {
                                        state.options_link_target_column_index = None;
                                        list_ui.memory_mut(|mem| mem.close_popup());
                                    }
                                    for (idx, header_name) in headers.iter() {
                                        if !current.is_empty() && !header_name.to_lowercase().contains(&current) { continue; }
                                        if list_ui.selectable_label(state.options_link_target_column_index == Some(*idx), header_name).clicked() {
                                            state.options_link_target_column_index = Some(*idx);
                                            list_ui.memory_mut(|mem| mem.close_popup());
                                        }
                                    }
                                });
                            },
                        );
                    });
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
                            // Always allow picking from the same-level sheet the metadata belongs to
                            let headers: Vec<String> = meta.columns.iter().map(|c| c.header.clone()).collect();
                            if headers.is_empty() { ui_h.label("<no headers>"); return; }
                            let sel_text = current_key.and_then(|i| headers.get(i)).cloned().unwrap_or_else(|| "(none)".to_string());
                            let combo_id = format!("key_parent_column_selector_internal_{}_{}", state.options_column_target_category.as_deref().unwrap_or(""), state.options_column_target_index);
                            let filter_key = format!("{}_filter", combo_id);
                            let button = ui_h.button(sel_text);
                            let popup_id = egui::Id::new(combo_id.clone());
                            if button.clicked() { ui_h.ctx().memory_mut(|mem| mem.open_popup(popup_id)); }
                            egui::containers::popup::popup_below_widget(
                                ui_h,
                                popup_id,
                                &button,
                                egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                                |popup_ui| {
                                    let mut filter_text = popup_ui.memory(|mem| mem.data.get_temp::<String>(filter_key.clone().into()).unwrap_or_default());
                                    let char_w = 8.0_f32;
                                    let max_name_len = headers.iter().map(|h| h.len()).max().unwrap_or(12);
                                    let padding = 24.0_f32;
                                    let mut popup_min_width = (max_name_len as f32) * char_w + padding;
                                    if popup_min_width < 120.0 { popup_min_width = 120.0; }
                                    if popup_min_width > 900.0 { popup_min_width = 900.0; }
                                    popup_ui.set_min_width(popup_min_width);
                                    popup_ui.horizontal(|ui_h2| {
                                        ui_h2.label("Filter:");
                                        let avail = ui_h2.available_width();
                                        let default_chars = 28usize;
                                        let desired = (default_chars as f32) * char_w;
                                        let width = desired.min(avail).min(popup_min_width - 40.0);
                                        let resp = ui_h2.add(egui::TextEdit::singleline(&mut filter_text).desired_width(width).hint_text("type to filter columns"));
                                        if resp.changed() { ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                                        if ui_h2.small_button("x").clicked() { filter_text.clear(); ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                                    });
                                    let current_filter = filter_text.to_lowercase();
                    egui::ScrollArea::vertical().max_height(300.0).show(popup_ui, |list_ui| {
                                        if list_ui.selectable_label(current_key.is_none(), "(none)").clicked() { current_key = None; list_ui.memory_mut(|mem| mem.close_popup()); }
                                        for (i, h) in headers.iter().enumerate() {
                        // Exclude the structure column itself only when the same sheet
                        if i == state.options_column_target_index { continue; }
                                            if !current_filter.is_empty() && !h.to_lowercase().contains(&current_filter) { continue; }
                                            if list_ui.selectable_label(current_key == Some(i), h).clicked() { current_key = Some(i); list_ui.memory_mut(|mem| mem.close_popup()); }
                                        }
                                    });
                                },
                            );
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
                        // For creation, the key must also be chosen from this sheet (same level)
                        let headers: Vec<String> = meta.columns.iter().map(|c| c.header.clone()).collect();
                        ui.horizontal(|ui_k| {
                            ui_k.label("Key Column:");
                            if headers.is_empty() { ui_k.label("<none>"); return; }
                            let mut temp_choice = state.options_structure_key_parent_column_temp;
                            let sel_text = temp_choice.and_then(|i| headers.get(i)).cloned().unwrap_or_else(|| "(none)".to_string());
                            let combo_id = format!("new_structure_key_parent_col_internal_{}_{}", state.options_column_target_category.as_deref().unwrap_or(""), state.options_column_target_index);
                            let filter_key = format!("{}_filter", combo_id);
                            let button = ui_k.button(sel_text);
                            let popup_id = egui::Id::new(combo_id.clone());
                            if button.clicked() { ui_k.ctx().memory_mut(|mem| mem.open_popup(popup_id)); }
                            egui::containers::popup::popup_below_widget(
                                ui_k,
                                popup_id,
                                &button,
                                egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                                |popup_ui| {
                                    let mut filter_text = popup_ui.memory(|mem| mem.data.get_temp::<String>(filter_key.clone().into()).unwrap_or_default());
                                    let char_w = 8.0_f32;
                                    let max_name_len = headers.iter().map(|h| h.len()).max().unwrap_or(12);
                                    let padding = 24.0_f32;
                                    let mut popup_min_width = (max_name_len as f32) * char_w + padding;
                                    if popup_min_width < 120.0 { popup_min_width = 120.0; }
                                    if popup_min_width > 900.0 { popup_min_width = 900.0; }
                                    popup_ui.set_min_width(popup_min_width);
                                    popup_ui.horizontal(|ui_h2| {
                                        ui_h2.label("Filter:");
                                        let avail = ui_h2.available_width();
                                        let default_chars = 28usize;
                                        let desired = (default_chars as f32) * char_w;
                                        let width = desired.min(avail).min(popup_min_width - 40.0);
                                        let resp = ui_h2.add(egui::TextEdit::singleline(&mut filter_text).desired_width(width).hint_text("type to filter columns"));
                                        if resp.changed() { ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                                        if ui_h2.small_button("x").clicked() { filter_text.clear(); ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                                    });
                                    let current_filter = filter_text.to_lowercase();
                                    egui::ScrollArea::vertical().max_height(300.0).show(popup_ui, |list_ui| {
                                        if list_ui.selectable_label(temp_choice.is_none(), "(none)").clicked() { temp_choice = None; list_ui.memory_mut(|mem| mem.close_popup()); }
                                        for (i,h) in headers.iter().enumerate() {
                                            if i == state.options_column_target_index { continue; }
                                            if !current_filter.is_empty() && !h.to_lowercase().contains(&current_filter) { continue; }
                                            if list_ui.selectable_label(temp_choice == Some(i), h).clicked() { temp_choice = Some(i); list_ui.memory_mut(|mem| mem.close_popup()); }
                                        }
                                    });
                                },
                            );
                            if temp_choice.is_some() { if ui_k.small_button("x").on_hover_text("Clear key selection").clicked() { temp_choice = None; } }
                            if temp_choice != state.options_structure_key_parent_column_temp { state.options_structure_key_parent_column_temp = temp_choice; }
                            ui_k.label("(Optional context only, not overwritten)");
                        });
                    }
                    ui.add(egui::Separator::default());
                    ui.label("Schema: choose source columns to copy into object fields.");
                    let (headers, _self_index) = meta_opt.map(|m| (m.columns.iter().map(|c| c.header.clone()).collect::<Vec<_>>(), state.options_column_target_index)).unwrap_or_default();
                    if state.options_structure_source_columns.is_empty() { state.options_structure_source_columns.push(None); }
                    let mut to_remove: Vec<usize> = Vec::new();
                    for i in 0..state.options_structure_source_columns.len() {
                        ui.horizontal(|ui_h| {
                            let mut val = state.options_structure_source_columns[i];
                            let sel_text = match val { Some(idx) => headers.get(idx).cloned().unwrap_or_else(|| "Invalid".to_string()), None => "None".to_string() };
                            let combo_id = format!("structure_src_col_internal_{}_{}", i, state.options_column_target_index);
                            let filter_key = format!("{}_filter", combo_id);
                            let btn = ui_h.button(sel_text);
                            let popup_id = egui::Id::new(combo_id.clone());
                            if btn.clicked() { ui_h.ctx().memory_mut(|mem| mem.open_popup(popup_id)); }
                            egui::containers::popup::popup_below_widget(
                                ui_h,
                                popup_id,
                                &btn,
                                egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                                |popup_ui| {
                                    let mut filter_text = popup_ui.memory(|mem| mem.data.get_temp::<String>(filter_key.clone().into()).unwrap_or_default());
                                    let char_w = 8.0_f32;
                                    let max_name_len = headers.iter().map(|h| h.len()).max().unwrap_or(12);
                                    let padding = 24.0_f32;
                                    let mut popup_min_width = (max_name_len as f32) * char_w + padding;
                                    if popup_min_width < 120.0 { popup_min_width = 120.0; }
                                    if popup_min_width > 900.0 { popup_min_width = 900.0; }
                                    popup_ui.set_min_width(popup_min_width);
                                    popup_ui.horizontal(|ui_h2| {
                                        ui_h2.label("Filter:");
                                        let avail = ui_h2.available_width();
                                        let default_chars = 28usize;
                                        let desired = (default_chars as f32) * char_w;
                                        let width = desired.min(avail).min(popup_min_width - 40.0);
                                        let resp = ui_h2.add(egui::TextEdit::singleline(&mut filter_text).desired_width(width).hint_text("type to filter columns"));
                                        if resp.changed() { ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                                        if ui_h2.small_button("x").clicked() { filter_text.clear(); ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), filter_text.clone())); }
                                    });
                                    let current_filter = filter_text.to_lowercase();
                                    egui::ScrollArea::vertical().max_height(300.0).show(popup_ui, |list_ui| {
                                        if list_ui.selectable_label(val.is_none(), "None").clicked() { val = None; list_ui.memory_mut(|mem| mem.close_popup()); }
                                        for (idx, header) in headers.iter().enumerate() {
                                            // Allow self-index selection now (was previously skipped)
                                            if !current_filter.is_empty() && !header.to_lowercase().contains(&current_filter) { continue; }
                                            if list_ui.selectable_label(val == Some(idx), header).clicked() { val = Some(idx); list_ui.memory_mut(|mem| mem.close_popup()); }
                                        }
                                    });
                                },
                            );
                            if val != state.options_structure_source_columns[i] { state.options_structure_source_columns[i] = val; }
                            if i + 1 < state.options_structure_source_columns.len() && ui_h.button("X").clicked() { to_remove.push(i); }
                        });
                    }
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
                let sources: Vec<usize> = state.options_structure_source_columns.iter().filter_map(|o| *o).collect();
                if sources.is_empty() { None } else { Some(sources) }
            } else { None },
            key_parent_column_index: if matches!(new_validator, Some(ColumnValidator::Structure)) { state.options_structure_key_parent_column_temp } else { None },
            original_self_validator: if matches!(new_validator, Some(ColumnValidator::Structure)) { current_validator.clone() } else { None },
        });
        // NOTE: key_parent_col captured; actual persistence must be handled by downstream event handler updating metadata, which should look at pending_structure_key_apply or similar. If such mechanism exists, it can be extended; for now we rely on separate pending_structure_key_apply logic elsewhere if needed.
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