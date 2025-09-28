// src/ui/elements/top_panel/mod.rs
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_tokio_tasks::TokioTasksRuntime;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{
    AiModeState, EditorWindowState, SheetInteractionState, ToyboxMode,
};
// Import the SheetEventWriters SystemParam struct
use crate::ui::elements::editor::ai_control_panel::show_ai_control_panel;
use crate::ui::elements::editor::main_editor::SheetEventWriters;
use crate::visual_copier::events::RequestAppExit;

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
                            // Gear button placed BEFORE the toybox mode picker per UX requirement
                            if ui_h.button("⚙").on_hover_text("Random Picker settings").clicked() {
                                // Pre-fill popup targets: use current active sheet
                                if let Some(s) = &active_sheet_opt { state.options_column_target_sheet = s.clone(); state.options_column_target_category = active_cat.clone(); }
                                state.show_random_picker_panel = true;
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
                                // Picker editing moved into the Random Picker popup (gear button). Do not show column name here.

                                // Refresh button positioned after the mode picker and before the result
                                if ui_h.add_enabled(active_sheet_opt.is_some(), egui::Button::new("🔄 Refresh")).clicked() {
                                    if let Some(sheet_name) = &active_sheet_opt {
                                        if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) {
                                            // Build candidate rows and compute weights from configured weight columns
                                            let result_col = state.random_simple_result_col;
                                            // Collect per-row weight as f64 by summing numeric parses across configured weight columns
                                            let weight_cols: Vec<usize> = state.random_picker_weight_columns.iter().filter_map(|o| *o).collect();
                                            let mut candidates: Vec<(usize, f64, &str)> = Vec::new();
                                            for (r_idx, row) in sheet.grid.iter().enumerate() {
                                                if let Some(val) = row.get(result_col) {
                                                    if val.trim().is_empty() { continue; }
                                                    // compute weight for this row
                                                    let mut wsum = 0.0f64;
                                                    for (wi, &wc) in weight_cols.iter().enumerate() {
                                                        if let Some(cell) = row.get(wc) {
                                                            if let Ok(n) = cell.trim().parse::<f64>() {
                                                                // apply exponent if configured, default 1.0
                                                                let exp = state.random_picker_weight_exponents.get(wi).cloned().unwrap_or(1.0);
                                                                   // apply multiplier if configured, default 1.0
                                                                   let mult = state.random_picker_weight_multipliers.get(wi).cloned().unwrap_or(1.0);
                                                                // Interpret exponent semantics: stored value map directly to power to apply to abs(n).
                                                                // For negative exponents we want sqrt-like behavior as described by the user: use 1/(n.powf(-exp))?
                                                                // Following user's description: when exponent = -2 => x^-2 = x^(1/2). We'll map stored e so that -2 -> 0.5.
                                                                // Use transform: applied_power = if exp < 0.0 { 1.0 / ( -exp ) } else { exp } ???
                                                                // Simpler: follow user's specification literally: exponent value 'e' maps to applied_power = if e<0 { 1.0/(-e) } else { e }
                                                                let applied_power = if exp < 0.0 { 1.0 / (-exp) } else { exp };
                                                                    let mut v = n.abs() * mult;
                                                                // guard for negative/zero
                                                                if v == 0.0 { v = 0.0; }
                                                                let weighted = v.powf(applied_power);
                                                                wsum += weighted;
                                                            }
                                                        }
                                                    }
                                                    // if no weight columns configured or all zeros, fallback to uniform weight 1
                                                    if weight_cols.is_empty() || wsum == 0.0 { wsum = 1.0; }
                                                    candidates.push((r_idx, wsum, val.as_str()));
                                                }
                                            }
                                            if candidates.is_empty() { state.random_picker_last_value.clear(); state.random_picker_copy_status.clear(); }
                                            else {
                                                // weighted random selection
                                                let total: f64 = candidates.iter().map(|c| c.1).sum();
                                                let mut target = rand::random::<f64>() * total;
                                                let mut chosen = None;
                                                for ( _r, w, v) in candidates.iter() {
                                                    if target <= *w { chosen = Some(*v); break; }
                                                    target -= *w;
                                                }
                                                if chosen.is_none() { chosen = Some(candidates.last().unwrap().2); }
                                                state.random_picker_last_value = chosen.unwrap().to_string();
                                                state.random_picker_copy_status.clear();
                                            }
                                            random_settings_changed = true;
                                        }
                                    }
                                }

                                // show value to the right of Refresh
                                let rp_value = state.random_picker_last_value.clone();
                                if !rp_value.is_empty() {
                                    let rp_resp = ui_h.add(egui::SelectableLabel::new(false, rp_value.clone()));
                                    if rp_resp.clicked() { ui_h.ctx().copy_text(rp_value.clone()); state.random_picker_copy_status = "Copied".to_string(); }
                                    if !state.random_picker_copy_status.is_empty() { ui_h.label(format!(" ({})", state.random_picker_copy_status)); }
                                } else { ui_h.label("<empty>"); }

                            } else {
                                // Summarizer controls inline on the same row (no extra label)
                            }
                            // Common inline after header_map2 selection when in Summarizer mode
                            if matches!(state.toybox_mode, ToyboxMode::Summarizer) {
                                // Build header map excluding Structure columns with data types
                                let mut header_map2: Vec<(usize, String, crate::sheets::definitions::ColumnDataType)> = Vec::new();
                                if let Some(sheet_name) = &active_sheet_opt { if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) { if let Some(meta) = &sheet.metadata { for (i,c) in meta.columns.iter().enumerate() { if !matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { header_map2.push((i, c.header.clone(), c.data_type)); } } } } }
                                if header_map2.is_empty() { ui_h.label("<no columns>"); return; }
                                let sel_idx = header_map2.iter().position(|(actual,_,_)| *actual == state.summarizer_selected_col).unwrap_or(0);
                                // Show selected column as a label; editing moved to popup
                                let selected_label = header_map2.get(sel_idx).map(|(_,h,_)| h.clone()).unwrap_or_else(|| "<no columns>".to_string());
                                ui_h.label(selected_label);
                                state.summarizer_selected_col = header_map2[sel_idx].0; // store actual index
                                // Compute button placed before result per UX
                                if ui_h.add_enabled(active_sheet_opt.is_some(), egui::Button::new("∑ Compute")).clicked() {
                                    state.summarizer_last_result.clear();
                                    if let Some(sheet_name) = &active_sheet_opt {
                                        if let Some(sheet) = registry.get_sheet(&active_cat, sheet_name) {
                                            // Combined sum across all selected summarizer columns
                                            let sel_cols: Vec<usize> = state.summarizer_selected_columns.iter().filter_map(|o| *o).collect();
                                            if sel_cols.is_empty() {
                                                state.summarizer_last_result = "<no columns>".to_string();
                                            } else {
                                                // Sum numeric columns (I64 or F64) and count numeric values; non-numeric are skipped
                                                let mut total_f64 = 0.0f64;
                                                let mut total_count = 0usize;
                                                let mut invalid = 0usize;
                                                for row in &sheet.grid {
                                                    for &col_index in sel_cols.iter() {
                                                        if let Some(val) = row.get(col_index) {
                                                            let s = val.trim(); if s.is_empty() { continue; }
                                                            // Try parse as f64 first
                                                            if let Ok(vf) = s.parse::<f64>() { total_f64 += vf; total_count += 1; }
                                                            else { invalid += 1; }
                                                        }
                                                    }
                                                }
                                                state.summarizer_last_result = format!("Sum: {:.4} (values: {}, invalid: {})", total_f64, total_count, invalid);
                                                state.summarizer_copy_status.clear();
                                            }
                                        }
                                    }
                                }
                                // Summarizer result read-only box; clickable to copy
                                let sum_value = state.summarizer_last_result.clone();
                                let sum_label = if sum_value.is_empty() { "<empty>".to_string() } else { sum_value.clone() };
                                let sum_resp = ui_h.add(egui::SelectableLabel::new(false, sum_label));
                                if sum_resp.clicked() && !sum_value.is_empty() {
                                    ui_h.ctx().copy_text(sum_value.clone());
                                    state.summarizer_copy_status = "Copied".to_string();
                                }
                                if !state.summarizer_copy_status.is_empty() { ui_h.label(format!(" ({})", state.summarizer_copy_status)); }
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
                            &mut sheet_writers.toggle_ai_row_generation,
                            &mut sheet_writers.update_ai_send_schema,
                            &mut sheet_writers.create_ai_schema_group,
                            &mut sheet_writers.rename_ai_schema_group,
                            &mut sheet_writers.select_ai_schema_group,
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
                                use crate::sheets::definitions::{RandomPickerSettings, RandomPickerMode, SheetMetadata};
                                // Ensure metadata exists for this sheet
                                if sheet_mut.metadata.is_none() {
                                    let num_cols = sheet_mut.grid.first().map(|r| r.len()).unwrap_or(0);
                                    let default_filename = format!("{}.json", sel);
                                    // active_cat is already an Option<String>
                                    sheet_mut.metadata = Some(SheetMetadata::create_generic(sel.clone(), default_filename, num_cols, active_cat.clone()));
                                }
                                if let Some(meta) = &mut sheet_mut.metadata {
                                    // collect weight columns and exponents from state
                                    let weight_cols: Vec<usize> = state.random_picker_weight_columns.iter().filter_map(|o| *o).collect();
                                    let weight_exps: Vec<f64> = state.random_picker_weight_exponents.iter().cloned().take(weight_cols.len()).collect();
                                    // Collect summarizer columns (ignore None)
                                    let summarizer_cols: Vec<usize> = state.summarizer_selected_columns.iter().filter_map(|o| *o).collect();
                                    let settings = if state.random_picker_mode_is_complex {
                                        RandomPickerSettings {
                                            mode: RandomPickerMode::Complex,
                                            simple_result_col_index: 0,
                                            complex_result_col_index: state.random_complex_result_col,
                                            // legacy single-index fields not used in new unified logic
                                            weight_col_index: None,
                                            second_weight_col_index: None,
                                            weight_columns: weight_cols.clone(),
                                            weight_exponents: weight_exps.clone(),
                                            weight_multipliers: state.random_picker_weight_multipliers.iter().cloned().take(weight_cols.len()).collect(),
                                            summarizer_columns: summarizer_cols.clone(),
                                        }
                                    } else {
                                        RandomPickerSettings {
                                            mode: RandomPickerMode::Simple,
                                            simple_result_col_index: state.random_simple_result_col,
                                            complex_result_col_index: 0,
                                            weight_col_index: None,
                                            second_weight_col_index: None,
                                            weight_columns: weight_cols.clone(),
                                            weight_exponents: weight_exps.clone(),
                                            weight_multipliers: state.random_picker_weight_multipliers.iter().cloned().take(weight_cols.len()).collect(),
                                            summarizer_columns: summarizer_cols.clone(),
                                        }
                                    };
                                    meta.random_picker = Some(settings.clone());
                                    meta_to_save = Some(meta.clone());
                                }
                            }
                            if let Some(m) = meta_to_save {
                                crate::sheets::systems::io::save::save_single_sheet(&*registry, &m);
                                if let Some(rp) = &m.random_picker { trace!("Random Picker saved (top panel) mode={:?} weights={} exps={} mults={} summarizers={}", rp.mode, rp.weight_columns.len(), rp.weight_exponents.len(), rp.weight_multipliers.len(), rp.summarizer_columns.len()); }
                            }
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
