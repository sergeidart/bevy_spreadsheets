// src/ui/elements/top_panel/mod.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;
// Import the SheetEventWriters SystemParam struct
use crate::ui::elements::editor::main_editor::SheetEventWriters;
use crate::visual_copier::{
    events::{
        PickFolderRequest, QueueTopPanelCopyEvent, RequestAppExit, ReverseTopPanelFoldersEvent,
        VisualCopierStateChanged,
    },
    resources::VisualCopierManager,
};

// Declare sub-modules
mod sheet_management_bar;
mod quick_copy_bar;
mod sheet_interaction_modes;
pub mod controls {
    pub mod delete_mode_panel;
}

// Re-export the main function that will be called by main_editor.rs
pub use self::orchestrator::show_top_panel_orchestrator;

pub(super) fn truncate_path_string(path_str: &str, max_width_pixels: f32, ui: &egui::Ui) -> String {
    if path_str.is_empty() {
        return "".to_string();
    }
    let font_id_val = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            path_str.to_string(),
            font_id_val.clone(),
            egui::Color32::PLACEHOLDER,
        )
    });

    if galley.size().x <= max_width_pixels {
        return path_str.to_string();
    }

    let ellipsis = "...";
    let ellipsis_width = ui.fonts(|f| {
        f.layout_no_wrap(
            ellipsis.to_string(),
            font_id_val.clone(),
            egui::Color32::PLACEHOLDER,
        )
    })
    .size()
    .x;

    if ellipsis_width > max_width_pixels {
        let mut fitting_ellipsis = String::new();
        let mut current_ellipsis_width = 0.0;
        for c in ellipsis.chars() {
            let char_s = c.to_string();
            let char_w = ui.fonts(|f| {
                f.layout_no_wrap(char_s.clone(), font_id_val.clone(), egui::Color32::PLACEHOLDER)
            })
            .size()
            .x;
            if current_ellipsis_width + char_w <= max_width_pixels {
                fitting_ellipsis.push(c);
                current_ellipsis_width += char_w;
            } else {
                break;
            }
        }
        return fitting_ellipsis;
    }

    let mut truncated_len = 0;
    let mut current_width = 0.0;

    for (idx, char_instance) in path_str.char_indices() {
        let char_s = match path_str.get(idx..idx + char_instance.len_utf8()) {
            Some(s) => s,
            None => break,
        };
        let char_w = ui.fonts(|f| {
            f.layout_no_wrap(char_s.to_string(), font_id_val.clone(), egui::Color32::PLACEHOLDER)
        })
        .size()
        .x;

        if current_width + char_w + ellipsis_width > max_width_pixels {
            break;
        }
        current_width += char_w;
        truncated_len = idx + char_instance.len_utf8();
    }

    if truncated_len == 0 && !path_str.is_empty() {
        return ellipsis.to_string();
    } else if path_str.is_empty() {
        return "".to_string();
    }

    format!("{}{}", &path_str[..truncated_len], ellipsis)
}

mod orchestrator {
    use super::*;
    // No extra imports needed here

    #[allow(clippy::too_many_arguments)]
    pub fn show_top_panel_orchestrator<'w>(
        ui: &mut egui::Ui,
        state: &mut EditorWindowState,
        registry: &mut SheetRegistry,
        sheet_writers: &mut SheetEventWriters<'w>, // Received as &mut
        mut copier_manager: ResMut<VisualCopierManager>,
        // MODIFIED: Make these EventWriter parameters mutable
        mut pick_folder_writer: EventWriter<'w, PickFolderRequest>,
        mut queue_top_panel_copy_writer: EventWriter<'w, QueueTopPanelCopyEvent>,
        mut reverse_folders_writer: EventWriter<'w, ReverseTopPanelFoldersEvent>,
        mut request_app_exit_writer: EventWriter<'w, RequestAppExit>,
        mut state_changed_writer: EventWriter<'w, VisualCopierStateChanged>,
    mut close_structure_writer: EventWriter<'w, crate::sheets::events::CloseStructureViewEvent>,
    ) {
        egui::TopBottomPanel::top("main_top_controls_panel_refactored")
            .show_inside(ui, |ui| {
                // First row: Back button if structure view active
                if !state.virtual_structure_stack.is_empty() {
                    ui.horizontal(|ui_back| {
                        if ui_back.button("⬅ Back").clicked() {
                            close_structure_writer.write(crate::sheets::events::CloseStructureViewEvent);
                        }
                    });
                }
                // Second row: standard sheet management controls (without Back)
                ui.horizontal(|ui_h| {
                    sheet_management_bar::show_sheet_management_controls(
                        ui_h,
                        state,
                        &*registry,
                        sheet_management_bar::SheetManagementEventWriters {
                             upload_req_writer: &mut sheet_writers.upload_req,
                             request_app_exit_writer: &mut request_app_exit_writer,
                             close_structure_writer: &mut close_structure_writer,
                        }
                    );
                });

                quick_copy_bar::show_quick_copy_controls(
                    ui,
                    state,
                    &mut copier_manager,
                    quick_copy_bar::QuickCopyEventWriters {
                        // MODIFIED: Pass &mut to local mutable EventWriters
                        pick_folder_writer: &mut pick_folder_writer,
                        queue_top_panel_copy_writer: &mut queue_top_panel_copy_writer,
                        reverse_folders_writer: &mut reverse_folders_writer,
                        state_changed_writer: &mut state_changed_writer,
                    },
                );
                ui.separator();

                ui.horizontal(|ui_h| {
                    sheet_interaction_modes::show_sheet_interaction_mode_buttons(
                        ui_h,
                        state,
                        &*registry,
                        sheet_interaction_modes::InteractionModeEventWriters {
                            add_row_event_writer: &mut sheet_writers.add_row,
                            add_column_event_writer: &mut sheet_writers.add_column,
                        }
                    );
                });
                // NEW: Random Picker panel expanded row (now virtual-structure aware)
                let (active_cat, active_sheet_opt) = state.current_sheet_context();
                if state.show_random_picker_panel {
                    ui.add_space(4.0);
                    let mut random_settings_changed = false;
                    ui.horizontal_wrapped(|ui_h| {
                        // Mode dropdown
                        ui_h.label("Random:");
                        let mut mode_is_complex = state.random_picker_mode_is_complex;
                        egui::ComboBox::from_id_salt("random_picker_mode")
                            .selected_text(if mode_is_complex { "Complex" } else { "Simple" })
                            .show_ui(ui_h, |ui| {
                                if ui.selectable_label(!mode_is_complex, "Simple").clicked() {
                                    mode_is_complex = false;
                                }
                                if ui.selectable_label(mode_is_complex, "Complex").clicked() {
                                    mode_is_complex = true;
                                }
                            });
                        if mode_is_complex != state.random_picker_mode_is_complex { state.random_picker_mode_is_complex = mode_is_complex; random_settings_changed = true; }

                        let is_enabled = active_sheet_opt.is_some();
                        // Refresh button will perform picking below based on mode

                        // Columns list
                        // Build selectable headers excluding Structure validator columns. Keep mapping to actual indices.
                        let mut header_map: Vec<(usize, String)> = Vec::new(); // (actual_col_index, header)
                        if let Some(sheet_name) = &active_sheet_opt {
                            if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) {
                                if let Some(meta) = &sheet.metadata {
                                    for (i, c) in meta.columns.iter().enumerate() {
                                        let is_structure = matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure));
                                        if !is_structure { header_map.push((i, c.header.clone())); }
                                    }
                                }
                            }
                        }
                        if header_map.is_empty() { ui_h.label("<no columns>"); return; }

                        if !state.random_picker_mode_is_complex {
                            // Simple
                            // Result column dropdown
                            // Map current actual index to selection index
                            let mut selection_idx = header_map.iter().position(|(actual, _)| *actual == state.random_simple_result_col).unwrap_or(0);
                            egui::ComboBox::from_id_salt("random_simple_result_col")
                                .selected_text(header_map.get(selection_idx).map(|(_,h)| h.clone()).unwrap_or_else(|| "<no columns>".to_string()))
                                .show_ui(ui_h, |ui| {
                                    for (i, (_actual, h)) in header_map.iter().enumerate() {
                                        if ui.selectable_label(i==selection_idx, h).clicked() { selection_idx = i; }
                                    }
                                });
                            let new_actual = header_map[selection_idx].0;
                            if new_actual != state.random_simple_result_col { state.random_simple_result_col = new_actual; random_settings_changed = true; }

                            // Read-only display field
                            ui_h.add_enabled(false, egui::TextEdit::singleline(&mut state.random_picker_last_value));

                            // Refresh button
                            if ui_h.add_enabled(is_enabled, egui::Button::new("🔄 Refresh")).clicked() {
                                if let Some(sheet_name) = &active_sheet_opt {
                                    if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) {
                                        let col = state.random_simple_result_col;
                                        let non_empty: Vec<&str> = sheet.grid.iter().filter_map(|row| row.get(col)).map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
                                        if non_empty.is_empty() { state.random_picker_last_value.clear(); } else { let idx = (rand::random::<u64>() as usize) % non_empty.len(); state.random_picker_last_value = non_empty[idx].to_string(); }
                                        random_settings_changed = true; }
                                }
                            }
                        } else {
                            // Complex
                            // Result col (mapping)
                            let mut res_sel_idx = header_map.iter().position(|(actual,_ )| *actual == state.random_complex_result_col).unwrap_or(0);
                            egui::ComboBox::from_id_salt("random_complex_result_col")
                                .selected_text(header_map.get(res_sel_idx).map(|(_,h)| h.clone()).unwrap_or_else(|| "<no columns>".to_string()))
                                .show_ui(ui_h, |ui| {
                                    for (i, (_a, h)) in header_map.iter().enumerate() { if ui.selectable_label(i==res_sel_idx, h).clicked() { res_sel_idx = i; } }
                                });
                            let new_res_actual = header_map[res_sel_idx].0;
                            if new_res_actual != state.random_complex_result_col { state.random_complex_result_col = new_res_actual; random_settings_changed = true; }
                            // Read-only display field for last picked value (before weights to distinguish roles)
                            ui_h.add_enabled(false, egui::TextEdit::singleline(&mut state.random_picker_last_value));

                            // Keep Refresh immediately after the result field (same relative spot as before)
                            if ui_h.add_enabled(is_enabled, egui::Button::new("🔄 Refresh")).clicked() {
                                if let Some(sheet_name) = &active_sheet_opt { if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) {
                                    let rcol = state.random_complex_result_col;
                                    let w1_idx = state.random_complex_weight_col;
                                    let w2_idx = state.random_complex_second_weight_col;
                                    let mut values: Vec<(&str, f64)> = Vec::new();
                                    for row in &sheet.grid { let val = row.get(rcol).map(|s| s.as_str()).unwrap_or(""); if val.is_empty() { continue; } let w1 = w1_idx.and_then(|i| row.get(i)).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0); let w2 = w2_idx.and_then(|i| row.get(i)).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0); let w = if w2_idx.is_some() { w1 + w2 } else { w1 }; if w > 0.0 { values.push((val, w)); } }
                                    if values.is_empty() { state.random_picker_last_value.clear(); } else { let total: f64 = values.iter().map(|(_, w)| *w).sum(); let mut target = rand::random::<f64>() * total; let mut picked = values[0].0; for (v, w) in values { if target <= w { picked = v; break; } target -= w; } state.random_picker_last_value = picked.to_string(); }
                                    random_settings_changed = true; } }
                            }

                            // First weight (mapping)
                            let mut w1_opt = state.random_complex_weight_col; // actual index
                            let mut w1_sel_idx = w1_opt.and_then(|a| header_map.iter().position(|(actual,_ )| *actual == a));
                            if w1_opt.is_some() && w1_sel_idx.is_none() { w1_opt = None; }
                            egui::ComboBox::from_id_salt("random_complex_weight_col")
                                .selected_text(w1_sel_idx.and_then(|si| header_map.get(si).map(|(_,h)| h.clone())).unwrap_or_else(|| "<no columns>".to_string()))
                                .show_ui(ui_h, |ui| { for (i, (_a,h)) in header_map.iter().enumerate() { if ui.selectable_label(Some(i)==w1_sel_idx, h).clicked() { w1_sel_idx = Some(i); w1_opt = Some(header_map[i].0); random_settings_changed = true; } } });
                            // Persist after first weight change
                            // (If None was previously, selecting sets Some)
                            // Note: Using currently set value
                            // Second weight (optional)
                            let mut w2_opt = state.random_complex_second_weight_col; // actual index
                            let mut w2_sel_idx = w2_opt.and_then(|a| header_map.iter().position(|(actual,_ )| *actual == a));
                            if w2_opt.is_some() && w2_sel_idx.is_none() { w2_opt = None; }
                            egui::ComboBox::from_id_salt("random_complex_second_weight_col")
                                .selected_text(if let Some(si)=w2_sel_idx { header_map.get(si).map(|(_,h)| h.clone()).unwrap_or_else(|| "(none)".to_string()) } else { "(none)".to_string() })
                                .show_ui(ui_h, |ui| {
                                    if ui.selectable_label(w2_sel_idx.is_none(), "(none)").clicked() { w2_opt=None; w2_sel_idx=None; random_settings_changed = true; }
                                    for (i, (_a,h)) in header_map.iter().enumerate() { if ui.selectable_label(Some(i)==w2_sel_idx, h).clicked(){ w2_sel_idx=Some(i); w2_opt=Some(header_map[i].0); random_settings_changed = true; } }
                                });
                            // end weights selection
                            // write back weight selections (actual indices)
                            if w1_opt != state.random_complex_weight_col { state.random_complex_weight_col = w1_opt; }
                            if w2_opt != state.random_complex_second_weight_col { state.random_complex_second_weight_col = w2_opt; }

                            // Refresh moved above
                        }
                    });
                    // Persist settings once after UI if changed
                    if random_settings_changed {
                        if let Some(sel) = &active_sheet_opt.clone() {
                            let mut meta_to_save = None;
                            if let Some(sheet_mut) = registry.get_sheet_mut(&active_cat, sel) {
                                if let Some(meta) = &mut sheet_mut.metadata {
                                    use crate::sheets::definitions::{RandomPickerSettings, RandomPickerMode};
                                    let settings = if state.random_picker_mode_is_complex {
                                        RandomPickerSettings {
                                            mode: RandomPickerMode::Complex,
                                            simple_result_col_index: 0,
                                            complex_result_col_index: state.random_complex_result_col,
                                            weight_col_index: state.random_complex_weight_col,
                                            second_weight_col_index: state.random_complex_second_weight_col,
                                        }
                                    } else {
                                        RandomPickerSettings {
                                            mode: RandomPickerMode::Simple,
                                            simple_result_col_index: state.random_simple_result_col,
                                            complex_result_col_index: 0,
                                            weight_col_index: None,
                                            second_weight_col_index: None,
                                        }
                                    };
                                    meta.random_picker = Some(settings.clone());
                                    meta_to_save = Some(meta.clone());
                                }
                            }
                            if let Some(m) = meta_to_save { crate::sheets::systems::io::save::save_single_sheet(&*registry, &m); }
                        }
                    }
                    ui.add_space(5.0);
                }
                // NEW: Summarizer panel expanded row
                if state.show_summarizer_panel {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui_h| {
                        ui_h.label("Summarize:");
                        // Collect headers and data types
                        let (active_cat, active_sheet_opt) = state.current_sheet_context();
                        // Build header map excluding Structure columns
                        let mut header_map: Vec<(usize, String, crate::sheets::definitions::ColumnDataType)> = Vec::new();
                        if let Some(sheet_name) = &active_sheet_opt { if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) { if let Some(meta) = &sheet.metadata { for (i,c) in meta.columns.iter().enumerate() { if !matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { header_map.push((i, c.header.clone(), c.data_type)); } } } } }
                        if header_map.is_empty() { ui_h.label("<no columns>"); return; }
                        // Map current actual index to selection index
                        let mut sel_idx = header_map.iter().position(|(actual,_,_)| *actual == state.summarizer_selected_col).unwrap_or(0);
                        egui::ComboBox::from_id_salt("summarizer_col")
                            .selected_text(header_map.get(sel_idx).map(|(_,h,_)| h.clone()).unwrap_or_else(|| "<no columns>".to_string()))
                            .show_ui(ui_h, |ui| { for (i, (_a,h,_dt)) in header_map.iter().enumerate() { if ui.selectable_label(i==sel_idx, h).clicked() { sel_idx = i; } } });
                        state.summarizer_selected_col = header_map[sel_idx].0; // store actual index
                        // Read-only result field
                        ui_h.add_enabled(false, egui::TextEdit::singleline(&mut state.summarizer_last_result));
                        // Compute button
                        if ui_h.add_enabled(active_sheet_opt.is_some(), egui::Button::new("∑ Compute")).clicked() {
                            state.summarizer_last_result.clear();
                            if let Some(sheet_name) = &active_sheet_opt {
                                if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) {
                                    let dtype = header_map.iter()
                                        .find(|(a,_,_)| *a == state.summarizer_selected_col)
                                        .map(|(_,_,dt)| *dt)
                                        .unwrap_or(crate::sheets::definitions::ColumnDataType::String);
                                    let col_index = state.summarizer_selected_col;
                                    match dtype {
                                        crate::sheets::definitions::ColumnDataType::String | crate::sheets::definitions::ColumnDataType::OptionString => {
                                            let count = sheet.grid.iter().filter_map(|row| row.get(col_index)).filter(|v| !v.trim().is_empty()).count();
                                            state.summarizer_last_result = format!("Count: {}", count);
                                        }
                                        crate::sheets::definitions::ColumnDataType::Bool | crate::sheets::definitions::ColumnDataType::OptionBool => {
                                            let (t,f) = sheet.grid.iter().filter_map(|row| row.get(col_index)).filter(|v| !v.trim().is_empty()).fold((0usize,0usize), |acc, v| { let vl = v.to_ascii_lowercase(); if vl=="true" || vl=="1" { (acc.0+1, acc.1) } else { (acc.0, acc.1+1) } });
                                            state.summarizer_last_result = format!("Bool Count -> true: {}, false: {}", t, f);
                                        }
                                        crate::sheets::definitions::ColumnDataType::U8 | crate::sheets::definitions::ColumnDataType::OptionU8 |
                                        crate::sheets::definitions::ColumnDataType::U16 | crate::sheets::definitions::ColumnDataType::OptionU16 |
                                        crate::sheets::definitions::ColumnDataType::U32 | crate::sheets::definitions::ColumnDataType::OptionU32 |
                                        crate::sheets::definitions::ColumnDataType::U64 | crate::sheets::definitions::ColumnDataType::OptionU64 => {
                                            let mut sum:u128=0; let mut count=0; let mut invalid=0; for row in &sheet.grid { if let Some(val)=row.get(col_index) { if val.trim().is_empty() {continue;} match val.parse::<u128>() { Ok(v)=>{sum+=v; count+=1;}, Err(_)=>invalid+=1 } } }
                                            state.summarizer_last_result = format!("Sum: {} (values: {}, invalid: {})", sum, count, invalid);
                                        }
                                        crate::sheets::definitions::ColumnDataType::I8 | crate::sheets::definitions::ColumnDataType::OptionI8 |
                                        crate::sheets::definitions::ColumnDataType::I16 | crate::sheets::definitions::ColumnDataType::OptionI16 |
                                        crate::sheets::definitions::ColumnDataType::I32 | crate::sheets::definitions::ColumnDataType::OptionI32 |
                                        crate::sheets::definitions::ColumnDataType::I64 | crate::sheets::definitions::ColumnDataType::OptionI64 => {
                                            let mut sum:i128=0; let mut count=0; let mut invalid=0; for row in &sheet.grid { if let Some(val)=row.get(col_index) { if val.trim().is_empty(){continue;} match val.parse::<i128>() { Ok(v)=>{sum+=v; count+=1;}, Err(_)=>invalid+=1 } } }
                                            state.summarizer_last_result = format!("Sum: {} (values: {}, invalid: {})", sum, count, invalid);
                                        }
                                        crate::sheets::definitions::ColumnDataType::F32 | crate::sheets::definitions::ColumnDataType::OptionF32 |
                                        crate::sheets::definitions::ColumnDataType::F64 | crate::sheets::definitions::ColumnDataType::OptionF64 => {
                                            let mut sum:f64=0.0; let mut count=0; let mut invalid=0; for row in &sheet.grid { if let Some(val)=row.get(col_index) { if val.trim().is_empty(){continue;} match val.parse::<f64>() { Ok(v)=>{sum+=v; count+=1;}, Err(_)=>invalid+=1 } } }
                                            state.summarizer_last_result = format!("Sum: {:.4} (values: {}, invalid: {})", sum, count, invalid);
                                        }
                                    }
                                }
                            }
                        }
                    });
                    ui.add_space(5.0);
                }
                ui.add_space(5.0);
            });
    }
}