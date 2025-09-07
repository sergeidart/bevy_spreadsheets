// src/ui/elements/top_panel/mod.rs
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, ToyboxMode, AiModeState, SheetInteractionState};
// Import the SheetEventWriters SystemParam struct
use crate::ui::elements::editor::main_editor::SheetEventWriters;
use crate::visual_copier::events::RequestAppExit;
use crate::ui::elements::editor::ai_control_panel::show_ai_control_panel;

// Declare sub-modules
pub mod sheet_management_bar;
// quick_copy_bar removed: Quick Copy UI now lives inside Settings popup
mod sheet_interaction_modes;
pub mod controls {
    pub mod delete_mode_panel;
}

// Re-export the main function that will be called by main_editor.rs
pub use self::orchestrator::show_top_panel_orchestrator;

// truncate_path_string helper removed with quick_copy_bar

mod orchestrator {
    use super::*;
    // No extra imports needed here
    
    // Calculate an approximate button width for a given text using current UI fonts and padding
    fn calc_button_width(ui: &egui::Ui, text: &str) -> f32 {
        let style = ui.style().clone();
        let font_id = egui::TextStyle::Button.resolve(&style);
        let text_w = ui.fonts(|f| {
            f.layout_no_wrap(text.to_owned(), font_id, style.visuals.text_color())
                .rect
                .width()
        });
        let pad = style.spacing.button_padding.x * 2.0;
        // add a tiny safety margin
        text_w + pad + 6.0
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show_top_panel_orchestrator<'w>(
        ui: &mut egui::Ui,
        state: &mut EditorWindowState,
        registry: &mut SheetRegistry,
        sheet_writers: &mut SheetEventWriters<'w>, // Received as &mut
    mut request_app_exit_writer: EventWriter<'w, RequestAppExit>,
    mut close_structure_writer: EventWriter<'w, crate::sheets::events::CloseStructureViewEvent>,
        runtime: &TokioTasksRuntime,
        session_api_key: &crate::SessionApiKey,
        commands: &mut Commands,
    ) {
        egui::TopBottomPanel::top("main_top_controls_panel_refactored")
            .show_inside(ui, |ui| {
                // Row 1: Back + mode toggles (left) and App Exit (right) rendered in a single horizontal row
                let row_h = ui.style().spacing.interact_size.y;
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_h), egui::Layout::left_to_right(egui::Align::Center), |ui_row| {
                    // Left group: Back + tool toggles
                    // Back button (reserve width when hidden to prevent jumps)
                    let back_width = calc_button_width(ui_row, "⬅ Back");
                    let interact_h = ui_row.style().spacing.interact_size.y;
                    if !state.virtual_structure_stack.is_empty() {
                        if ui_row.add_sized([back_width, interact_h], egui::Button::new("⬅ Back")).clicked() {
                            close_structure_writer.write(crate::sheets::events::CloseStructureViewEvent);
                        }
                        ui_row.add_space(6.0);
                    } else {
                        ui_row.allocate_exact_size(egui::vec2(back_width, interact_h), egui::Sense::hover());
                        ui_row.add_space(6.0);
                    }
                    sheet_interaction_modes::show_sheet_interaction_mode_buttons(
                        ui_row,
                        state,
                        &*registry,
                    );
                    // Right group: place App Exit at far-right by allocating the remaining width to a right-to-left child
                    let remaining_w = ui_row.available_width();
                    ui_row.allocate_ui_with_layout(
                        egui::vec2(remaining_w, row_h),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |r| {
                            r.add_space(12.0);
                            if r.add(egui::Button::new("❌ App Exit")).clicked() {
                                info!("'App Exit' button clicked. Sending RequestAppExit event.");
                                request_app_exit_writer.write(RequestAppExit);
                            }
                        },
                    );
                });

                // Spacing between rows (slightly tighter)
                ui.add_space(3.0);

                // Row 2: Left side shows expanded Toybox / AI / Delete content; Right side shows Settings
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_h), egui::Layout::left_to_right(egui::Align::Center), |ui_row| {
                    // LEFT: Expanded content
                    // Toybox expanded content inline
                    let (active_cat, active_sheet_opt) = state.current_sheet_context();
                    let mut random_settings_changed = false;
                    if state.show_toybox_menu {
                        ui_row.horizontal_wrapped(|ui_h| {
                            // Indent to align under Toybox toggle
                            if state.last_toybox_button_min_x > 0.0 {
                                let panel_left = ui_h.max_rect().min.x;
                                let indent = (state.last_toybox_button_min_x - panel_left).max(0.0);
                                ui_h.add_space(indent);
                            }
                            // Toybox mode dropdown (no extra label to avoid duplication)
                            let mut selected_mode = state.toybox_mode;
                            egui::ComboBox::from_id_salt("toybox_mode_picker")
                                .selected_text(match selected_mode { ToyboxMode::Randomizer => "Randomizer", ToyboxMode::Summarizer => "Summarizer" })
                                .show_ui(ui_h, |ui| {
                                    ui.selectable_value(&mut selected_mode, ToyboxMode::Randomizer, "Randomizer");
                                    ui.selectable_value(&mut selected_mode, ToyboxMode::Summarizer, "Summarizer");
                                });
                            if selected_mode != state.toybox_mode { state.toybox_mode = selected_mode; }
                            // Show only the chosen tool inline
                            if matches!(state.toybox_mode, ToyboxMode::Randomizer) {
                                let mut mode_is_complex = state.random_picker_mode_is_complex;
                                egui::ComboBox::from_id_salt("random_picker_mode")
                                    .selected_text(if mode_is_complex { "Complex" } else { "Simple" })
                                    .show_ui(ui_h, |ui| {
                                        if ui.selectable_label(!mode_is_complex, "Simple").clicked() { mode_is_complex = false; }
                                        if ui.selectable_label(mode_is_complex, "Complex").clicked() { mode_is_complex = true; }
                                    });
                                if mode_is_complex != state.random_picker_mode_is_complex { state.random_picker_mode_is_complex = mode_is_complex; random_settings_changed = true; }

                                let is_enabled = active_sheet_opt.is_some();
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
                                    // Simple mode
                                    let mut selection_idx = header_map.iter().position(|(actual, _)| *actual == state.random_simple_result_col).unwrap_or(0);
                                    egui::ComboBox::from_id_salt("random_simple_result_col")
                                        .selected_text(header_map.get(selection_idx).map(|(_,h)| h.clone()).unwrap_or_else(|| "<no columns>".to_string()))
                                        .show_ui(ui_h, |ui| {
                                            for (i, (_actual, h)) in header_map.iter().enumerate() { if ui.selectable_label(i==selection_idx, h).clicked() { selection_idx = i; } }
                                        });
                                    let new_actual = header_map[selection_idx].0;
                                    if new_actual != state.random_simple_result_col { state.random_simple_result_col = new_actual; random_settings_changed = true; }
                                    ui_h.add_enabled(false, egui::TextEdit::singleline(&mut state.random_picker_last_value));
                                    if ui_h.add_enabled(is_enabled, egui::Button::new("🔄 Refresh")).clicked() {
                                        if let Some(sheet_name) = &active_sheet_opt { if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) {
                                            let col = state.random_simple_result_col;
                                            let non_empty: Vec<&str> = sheet.grid.iter().filter_map(|row| row.get(col)).map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
                                            if non_empty.is_empty() { state.random_picker_last_value.clear(); }
                                            else { let idx = (rand::random::<u64>() as usize) % non_empty.len(); state.random_picker_last_value = non_empty[idx].to_string(); }
                                            random_settings_changed = true; } }
                                    }
                                } else {
                                    // Complex mode
                                    let mut res_sel_idx = header_map.iter().position(|(actual,_ )| *actual == state.random_complex_result_col).unwrap_or(0);
                                    egui::ComboBox::from_id_salt("random_complex_result_col")
                                        .selected_text(header_map.get(res_sel_idx).map(|(_,h)| h.clone()).unwrap_or_else(|| "<no columns>".to_string()))
                                        .show_ui(ui_h, |ui| { for (i, (_a, h)) in header_map.iter().enumerate() { if ui.selectable_label(i==res_sel_idx, h).clicked() { res_sel_idx = i; } } });
                                    let new_res_actual = header_map[res_sel_idx].0;
                                    if new_res_actual != state.random_complex_result_col { state.random_complex_result_col = new_res_actual; random_settings_changed = true; }
                                    ui_h.add_enabled(false, egui::TextEdit::singleline(&mut state.random_picker_last_value));
                                    if ui_h.add_enabled(is_enabled, egui::Button::new("🔄 Refresh")).clicked() {
                                        if let Some(sheet_name) = &active_sheet_opt { if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) {
                                            let rcol = state.random_complex_result_col;
                                            let w1_idx = state.random_complex_weight_col;
                                            let w2_idx = state.random_complex_second_weight_col;
                                            let mut values: Vec<(&str, f64)> = Vec::new();
                                            for row in &sheet.grid {
                                                let val = row.get(rcol).map(|s| s.as_str()).unwrap_or("");
                                                if val.is_empty() { continue; }
                                                let w1 = w1_idx.and_then(|i| row.get(i)).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                                let w2 = w2_idx.and_then(|i| row.get(i)).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                                let w = if w2_idx.is_some() { w1 + w2 } else { w1 };
                                                if w > 0.0 { values.push((val, w)); }
                                            }
                                            if values.is_empty() { state.random_picker_last_value.clear(); }
                                            else {
                                                let total: f64 = values.iter().map(|(_, w)| *w).sum();
                                                let mut target = rand::random::<f64>() * total;
                                                let mut picked = values[0].0;
                                                for (v, w) in values { if target <= w { picked = v; break; } target -= w; }
                                                state.random_picker_last_value = picked.to_string();
                                            }
                                            random_settings_changed = true; } }
                                    }
                                    let mut w1_opt = state.random_complex_weight_col; // actual index
                                    let mut w1_sel_idx = w1_opt.and_then(|a| header_map.iter().position(|(actual,_ )| *actual == a));
                                    if w1_opt.is_some() && w1_sel_idx.is_none() { w1_opt = None; }
                                    egui::ComboBox::from_id_salt("random_complex_weight_col")
                                        .selected_text(w1_sel_idx.and_then(|si| header_map.get(si).map(|(_,h)| h.clone())).unwrap_or_else(|| "<no columns>".to_string()))
                                        .show_ui(ui_h, |ui| { for (i, (_a,h)) in header_map.iter().enumerate() { if ui.selectable_label(Some(i)==w1_sel_idx, h).clicked() { w1_sel_idx = Some(i); w1_opt = Some(header_map[i].0); random_settings_changed = true; } } });
                                    let mut w2_opt = state.random_complex_second_weight_col; // actual index
                                    let mut w2_sel_idx = w2_opt.and_then(|a| header_map.iter().position(|(actual,_ )| *actual == a));
                                    if w2_opt.is_some() && w2_sel_idx.is_none() { w2_opt = None; }
                                    egui::ComboBox::from_id_salt("random_complex_second_weight_col")
                                        .selected_text(if let Some(si)=w2_sel_idx { header_map.get(si).map(|(_,h)| h.clone()).unwrap_or_else(|| "(none)".to_string()) } else { "(none)".to_string() })
                                        .show_ui(ui_h, |ui| {
                                            if ui.selectable_label(w2_sel_idx.is_none(), "(none)").clicked() { w2_opt=None; w2_sel_idx=None; random_settings_changed = true; }
                                            for (i, (_a,h)) in header_map.iter().enumerate() { if ui.selectable_label(Some(i)==w2_sel_idx, h).clicked(){ w2_sel_idx=Some(i); w2_opt=Some(header_map[i].0); random_settings_changed = true; } }
                                        });
                                    if w1_opt != state.random_complex_weight_col { state.random_complex_weight_col = w1_opt; }
                                    if w2_opt != state.random_complex_second_weight_col { state.random_complex_second_weight_col = w2_opt; }
                                }

                            } else {
                                // Summarizer controls inline on the same row (no extra label)
                            }
                            // Common inline after header_map2 selection when in Summarizer mode
                            if matches!(state.toybox_mode, ToyboxMode::Summarizer) {
                                // Build header map excluding Structure columns with data types
                                let mut header_map2: Vec<(usize, String, crate::sheets::definitions::ColumnDataType)> = Vec::new();
                                if let Some(sheet_name) = &active_sheet_opt { if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) { if let Some(meta) = &sheet.metadata { for (i,c) in meta.columns.iter().enumerate() { if !matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { header_map2.push((i, c.header.clone(), c.data_type)); } } } } }
                                if header_map2.is_empty() { ui_h.label("<no columns>"); return; }
                                let mut sel_idx = header_map2.iter().position(|(actual,_,_)| *actual == state.summarizer_selected_col).unwrap_or(0);
                                egui::ComboBox::from_id_salt("summarizer_col")
                                    .selected_text(header_map2.get(sel_idx).map(|(_,h,_)| h.clone()).unwrap_or_else(|| "<no columns>".to_string()))
                                    .show_ui(ui_h, |ui| { for (i, (_a,h,_dt)) in header_map2.iter().enumerate() { if ui.selectable_label(i==sel_idx, h).clicked() { sel_idx = i; } } });
                                state.summarizer_selected_col = header_map2[sel_idx].0; // store actual index
                                ui_h.add_enabled(false, egui::TextEdit::singleline(&mut state.summarizer_last_result));
                                if ui_h.add_enabled(active_sheet_opt.is_some(), egui::Button::new("∑ Compute")).clicked() {
                                    state.summarizer_last_result.clear();
                                    if let Some(sheet_name) = &active_sheet_opt { if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) {
                                        let dtype = header_map2.iter().find(|(a,_,_)| *a == state.summarizer_selected_col).map(|(_,_,dt)| *dt).unwrap_or(crate::sheets::definitions::ColumnDataType::String);
                                        let col_index = state.summarizer_selected_col;
                                        match dtype {
                                            crate::sheets::definitions::ColumnDataType::String => {
                                                let count = sheet.grid.iter().filter_map(|row| row.get(col_index)).filter(|v| !v.trim().is_empty()).count();
                                                state.summarizer_last_result = format!("Count: {}", count);
                                            }
                                            crate::sheets::definitions::ColumnDataType::Bool => {
                                                let (t,f) = sheet.grid.iter().filter_map(|row| row.get(col_index)).filter(|v| !v.trim().is_empty()).fold((0usize,0usize), |acc, v| { let vl = v.to_ascii_lowercase(); if vl=="true" || vl=="1" { (acc.0+1, acc.1) } else { (acc.0, acc.1+1) } });
                                                state.summarizer_last_result = format!("Bool Count -> true: {}, false: {}", t, f);
                                            }
                                            crate::sheets::definitions::ColumnDataType::I64 => {
                                                let mut sum:i128=0; let mut count=0; let mut invalid=0; for row in &sheet.grid { if let Some(val)=row.get(col_index) { if val.trim().is_empty(){continue;} match val.parse::<i128>() { Ok(v)=>{sum+=v; count+=1;}, Err(_)=>invalid+=1 } } }
                                                state.summarizer_last_result = format!("Sum: {} (values: {}, invalid: {})", sum, count, invalid);
                                            }
                                            crate::sheets::definitions::ColumnDataType::F64 => {
                                                let mut sum:f64=0.0; let mut count=0; let mut invalid=0; for row in &sheet.grid { if let Some(val)=row.get(col_index) { if val.trim().is_empty(){continue;} match val.parse::<f64>() { Ok(v)=>{sum+=v; count+=1;}, Err(_)=>invalid+=1 } } }
                                                state.summarizer_last_result = format!("Sum: {:.4} (values: {}, invalid: {})", sum, count, invalid);
                                            }
                                        }
                                    } }
                                }
                            }
                        });
                    }

                    // AI/Delete panels inline on the second row under their toggles
                    if state.current_interaction_mode == SheetInteractionState::AiModeActive &&
                        matches!(state.ai_mode, AiModeState::Preparing | AiModeState::Submitting | AiModeState::ResultsReady)
                    {
                        let current_category = state.selected_category.clone();
                        let current_sheet = state.selected_sheet_name.clone();
                        show_ai_control_panel(
                            ui_row,
                            state,
                            &current_category,
                            &current_sheet,
                            runtime,
                            &*registry,
                            commands,
                            session_api_key,
                        );
                    }
                    if state.current_interaction_mode == SheetInteractionState::DeleteModeActive {
                        controls::delete_mode_panel::show_delete_mode_active_controls(
                            ui_row,
                            state,
                            controls::delete_mode_panel::DeleteModeEventWriters {
                                delete_rows_event_writer: &mut sheet_writers.delete_rows,
                                delete_columns_event_writer: &mut sheet_writers.delete_columns,
                            }
                        );
                    }

                    // Persist Random Picker settings once after UI if changed
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

                    // RIGHT: Settings pinned to far-right
                    let remaining_w = ui_row.available_width();
                    ui_row.allocate_ui_with_layout(
                        egui::vec2(remaining_w, row_h),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |r| {
                            r.add_space(12.0);
                            if r
                                .button("⚙ Settings")
                                .on_hover_text("Open Settings")
                                .clicked()
                            {
                                state.show_settings_popup = true;
                            }
                        },
                    );
                });

                // Removed filler alignment row to prevent extra row
                ui.add_space(5.0);
            });
    }
}