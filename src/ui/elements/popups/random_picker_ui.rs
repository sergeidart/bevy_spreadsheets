use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, ToyboxMode};
use crate::ui::validation::normalize_for_link_cmp;
use bevy::prelude::*; // Keep bevy prelude
use bevy_egui::egui;

pub(super) struct RandomPickerUiResult {
    pub apply_clicked: bool,
    pub cancel_clicked: bool,
    pub close_via_x: bool,
}

/// Simple popup to select result column and optional weight columns for Random Picker
pub(super) fn show_random_picker_window_ui(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry_immut: &SheetRegistry,
) -> RandomPickerUiResult {
    let mut popup_open = state.show_random_picker_panel;
    let mut apply_clicked = false;
    let mut cancel_clicked = false;

    let popup_category = state.options_column_target_category.clone();
    let popup_sheet_name = state.options_column_target_sheet.clone();

    // Attempt hydration of runtime picker state from metadata if empty/default
    if let Some(sheet) = registry_immut.get_sheet(&popup_category, &popup_sheet_name) {
        if let Some(meta) = &sheet.metadata {
            if let Some(rp) = &meta.random_picker {
                // Detect empty runtime state (no real weights yet)
                let empty_runtime = state
                    .random_picker_weight_columns
                    .iter()
                    .all(|o| o.is_none())
                    && state.random_picker_weight_columns.len() <= 2;
                if empty_runtime && !rp.weight_columns.is_empty() {
                    state.random_picker_weight_columns.clear();
                    state.random_picker_weight_exponents.clear();
                    state.random_picker_weight_multipliers.clear();
                    for (i, &ci) in rp.weight_columns.iter().enumerate() {
                        state.random_picker_weight_columns.push(Some(ci));
                        let exp = rp.weight_exponents.get(i).copied().unwrap_or(1.0);
                        let mul = rp.weight_multipliers.get(i).copied().unwrap_or(1.0);
                        state.random_picker_weight_exponents.push(exp);
                        state.random_picker_weight_multipliers.push(mul);
                    }
                    // trailing None slot for UI
                    state.random_picker_weight_columns.push(None);
                    state.random_picker_weight_exponents.push(1.0);
                    state.random_picker_weight_multipliers.push(1.0);
                }
                // Hydrate summarizers similarly if empty
                let empty_summ = state
                    .summarizer_selected_columns
                    .iter()
                    .all(|o| o.is_none())
                    || state.summarizer_selected_columns.is_empty();
                if empty_summ && !rp.summarizer_columns.is_empty() {
                    state.summarizer_selected_columns.clear();
                    for &ci in rp.summarizer_columns.iter() {
                        state.summarizer_selected_columns.push(Some(ci));
                    }
                    state.summarizer_selected_columns.push(None);
                }
                // Result column hydration
                if matches!(
                    rp.mode,
                    crate::sheets::definitions::RandomPickerMode::Simple
                ) {
                    state.random_simple_result_col = rp.simple_result_col_index;
                } else {
                    state.random_simple_result_col = rp.complex_result_col_index;
                    // fallback
                }
            }
        }
    }

    egui::Window::new("Random Picker Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut popup_open)
        .show(ctx, |ui| {
            // Build header list
            let headers: Vec<String> = registry_immut.get_sheet(&popup_category, &popup_sheet_name)
                .and_then(|s| s.metadata.as_ref())
                .map(|m| m.columns.iter().map(|c| c.header.clone()).collect())
                .unwrap_or_default();

            if headers.is_empty() {
                ui.label("No columns available");
            } else {
                let is_random = matches!(state.toybox_mode, ToyboxMode::Randomizer);
                // Data Column only for Randomizer
                if is_random {
                    // Ensure the multi-term filter terms vector has at least one entry
                    if state.options_column_filter_terms.is_empty() { state.options_column_filter_terms.push(String::new()); }

                    // Build lowercase trimmed term list (ignore empties)
                    let terms: Vec<String> = state
                        .options_column_filter_terms
                        .iter()
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect();

                    // Filter headers by matching any term (OR). If no terms, include all.
                    let filtered: Vec<(usize, String)> = headers
                        .iter()
                        .enumerate()
                        .filter_map(|(i, h)| {
                            if terms.is_empty() {
                                Some((i, h.clone()))
                            } else {
                                let lower = h.to_lowercase();
                                if terms.iter().any(|t| lower.contains(t)) { Some((i, h.clone())) } else { None }
                            }
                        })
                        .collect();

                    ui.horizontal(|ui_h| {
                        ui_h.label("Data Column:");
                        // Use button + popup with per-popup filter like Column Options
                        if filtered.is_empty() {
                            ui_h.label("<no matching columns>");
                        } else {
                            // current selection display
                            let display = headers.get(state.random_simple_result_col).cloned().unwrap_or_else(|| "--Select--".to_string());
                            let combo_id = format!("rp_data_col_combo_{}", popup_sheet_name);
                            let filter_key = format!("{}_filter", combo_id);
                            let btn = ui_h.button(display);
                            let popup_id = egui::Id::new(combo_id.clone());
                            if btn.clicked() { ui_h.ctx().memory_mut(|mem| mem.open_popup(popup_id)); }
                            egui::containers::popup::popup_below_widget(
                                ui_h,
                                popup_id,
                                &btn,
                                egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                                |popup_ui| {
                                    let mut ftext = popup_ui.memory(|mem| mem.data.get_temp::<String>(filter_key.clone().into()).unwrap_or_default());
                                    let char_w = 8.0_f32;
                                    let max_name_len = headers.iter().map(|h| h.len()).max().unwrap_or(12);
                                    let mut popup_min_width = (max_name_len as f32) * char_w + 24.0;
                                    popup_min_width = popup_min_width.clamp(160.0, 900.0);
                                    popup_ui.set_min_width(popup_min_width);
                                    popup_ui.horizontal(|ui_h2| {
                                        ui_h2.label("Filter:");
                                        let avail = ui_h2.available_width();
                                        let default_chars = 28usize;
                                        let desired = (default_chars as f32) * char_w;
                                        let width = desired.min(avail).min(popup_min_width - 40.0);
                                        let resp = ui_h2.add(egui::TextEdit::singleline(&mut ftext).desired_width(width).hint_text("type to filter columns"));
                                        if resp.changed() { ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), ftext.clone())); }
                                        if ui_h2.small_button("x").clicked() { ftext.clear(); ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), ftext.clone())); }
                                    });
                                    let current = normalize_for_link_cmp(&ftext);
                                    egui::ScrollArea::vertical().max_height(300.0).show(popup_ui, |list_ui| {
                                        if list_ui.selectable_label(false, "--Select--").clicked() {
                                            state.random_simple_result_col = 0usize; // default to first
                                            list_ui.memory_mut(|mem| mem.close_popup());
                                        }
                                        for (idx, h) in headers.iter().enumerate() {
                                            if !current.is_empty() && !normalize_for_link_cmp(h).contains(&current) { continue; }
                                            if list_ui.selectable_label(state.random_simple_result_col == idx, h).clicked() {
                                                state.random_simple_result_col = idx;
                                                list_ui.memory_mut(|mem| mem.close_popup());
                                            }
                                        }
                                    });
                                },
                            );
                        }
                    });

                    // Note: per-popup filtering is available within each picker popup; global "contains fragment" terms removed.
                    // Separator between Data Column section and Weight Columns (as requested)
                    ui.separator();
                    // Weight columns for Randomizer
                    ui.label("Weight Columns:");
                    if state.random_picker_weight_columns.is_empty() { state.random_picker_weight_columns.push(None); state.random_picker_weight_exponents.push(1.0); state.random_picker_weight_multipliers.push(1.0); }
                    let mut remove_indices: Vec<usize> = Vec::new();
                    for i in 0..state.random_picker_weight_columns.len() {
                        ui.horizontal(|ui_h| {
                            let current_opt = state.random_picker_weight_columns[i];
                            let current_label = current_opt.and_then(|ci| headers.get(ci).cloned()).unwrap_or_else(|| "(none)".to_string());
                            // Layout: ( Title picker * multiplier ) ^ exponent  [x]
                            // Picker button
                            if current_opt.is_some() { ui_h.label("("); }
                            let combo_id = format!("rp_weight_dyn_{}_combo_{}_{}", i, popup_sheet_name, state.options_column_target_index);
                            let filter_key = format!("{}_filter", combo_id);
                            let btn = ui_h.button(current_label.clone());
                            let popup_id = egui::Id::new(combo_id.clone());
                            if btn.clicked() { ui_h.ctx().memory_mut(|mem| mem.open_popup(popup_id)); }
                            egui::containers::popup::popup_below_widget(
                                ui_h,
                                popup_id,
                                &btn,
                                egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                                |popup_ui| {
                                    let mut ftext = popup_ui.memory(|mem| mem.data.get_temp::<String>(filter_key.clone().into()).unwrap_or_default());
                                    let char_w = 8.0_f32;
                                    let max_name_len = headers.iter().map(|h| h.len()).max().unwrap_or(12);
                                    let mut popup_min_width = (max_name_len as f32) * char_w + 24.0;
                                    popup_min_width = popup_min_width.clamp(120.0, 900.0);
                                    popup_ui.set_min_width(popup_min_width);
                                    popup_ui.horizontal(|ui_h2| {
                                        ui_h2.label("Filter:");
                                        let avail = ui_h2.available_width();
                                        let default_chars = 28usize;
                                        let desired = (default_chars as f32) * char_w;
                                        let width = desired.min(avail).min(popup_min_width - 40.0);
                                        let resp = ui_h2.add(egui::TextEdit::singleline(&mut ftext).desired_width(width).hint_text("type to filter columns"));
                                        if resp.changed() { ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), ftext.clone())); }
                                        if ui_h2.small_button("x").clicked() { ftext.clear(); ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), ftext.clone())); }
                                    });
                                    let current = normalize_for_link_cmp(&ftext);
                                    egui::ScrollArea::vertical().max_height(300.0).show(popup_ui, |list_ui| {
                                        if list_ui.selectable_label(current_opt.is_none(), "(none)").clicked() { state.random_picker_weight_columns[i] = None; list_ui.memory_mut(|mem| mem.close_popup()); }
                                        for (idx, h) in headers.iter().enumerate() {
                                            if !current.is_empty() && !normalize_for_link_cmp(h).contains(&current) { continue; }
                                            if list_ui.selectable_label(current_opt == Some(idx), h).clicked() { state.random_picker_weight_columns[i] = Some(idx); list_ui.memory_mut(|mem| mem.close_popup()); }
                                        }
                                    });
                                },
                            );
                            // If a weight column is selected, show multiplier and exponent controls; otherwise hide them for simplicity
                            if current_opt.is_some() {
                                if i >= state.random_picker_weight_multipliers.len() { state.random_picker_weight_multipliers.resize(i+1, 1.0); }
                                let mut mult_val = state.random_picker_weight_multipliers[i];
                                ui_h.label(" * ");
                                let mv = egui::DragValue::new(&mut mult_val).speed(0.1).range(0.0..=1e6);
                                let mresp = ui_h.add_sized([56.0, 20.0], mv);
                                mresp.on_hover_text("Per-weight linear multiplier (applied before exponent). Default 1.");
                                state.random_picker_weight_multipliers[i] = mult_val;
                                // Show ' )^ exponent ' compactly: caret label then exponent box
                                if i >= state.random_picker_weight_exponents.len() { state.random_picker_weight_exponents.resize(i+1, 1.0); }
                                let mut exp_val = state.random_picker_weight_exponents[i];
                                ui_h.label(")");
                                ui_h.label("^");
                                let dv = egui::DragValue::new(&mut exp_val).speed(0.1).range(-10.0..=10.0);
                                let resp = ui_h.add_sized([48.0, 20.0], dv);
                                resp.on_hover_text("Per-weight exponent. Default 1. Negative values map to root behavior per UX (e.g. -2 -> sqrt).");
                                state.random_picker_weight_exponents[i] = exp_val;
                            }
                            // Remove button
                            if ui_h.small_button("x").on_hover_text("Remove").clicked() { remove_indices.push(i); }
                        });
                    }
                    // Remove requested indices from all parallel vectors
                    for &idx in remove_indices.iter().rev() {
                        if idx < state.random_picker_weight_columns.len() { state.random_picker_weight_columns.remove(idx); }
                        if idx < state.random_picker_weight_exponents.len() { state.random_picker_weight_exponents.remove(idx); }
                        if idx < state.random_picker_weight_multipliers.len() { state.random_picker_weight_multipliers.remove(idx); }
                    }
                    // Compact out intermediate None entries (but keep at least one trailing None slot)
                    let mut compacted_cols: Vec<Option<usize>> = Vec::new();
                    let mut compacted_exps: Vec<f64> = Vec::new();
                    let mut compacted_mults: Vec<f64> = Vec::new();
                    for (i, opt) in state.random_picker_weight_columns.iter().enumerate() {
                        if opt.is_some() { compacted_cols.push(*opt); compacted_exps.push(state.random_picker_weight_exponents.get(i).cloned().unwrap_or(1.0)); compacted_mults.push(state.random_picker_weight_multipliers.get(i).cloned().unwrap_or(1.0)); }
                    }
                    // restore and ensure one trailing None slot
                    state.random_picker_weight_columns = compacted_cols;
                    state.random_picker_weight_exponents = compacted_exps;
                    state.random_picker_weight_multipliers = compacted_mults;
                    if state.random_picker_weight_columns.is_empty() { state.random_picker_weight_columns.push(None); state.random_picker_weight_exponents.push(1.0); state.random_picker_weight_multipliers.push(1.0); }
                    if !state.random_picker_weight_columns.last().map(|o| o.is_none()).unwrap_or(false) { state.random_picker_weight_columns.push(None); state.random_picker_weight_exponents.push(1.0); state.random_picker_weight_multipliers.push(1.0); }
                }

                // Summarizer section only when Summarizer picked
                    if matches!(state.toybox_mode, ToyboxMode::Summarizer) {
                    ui.label("Summarizer Columns:");
                    if state.summarizer_selected_columns.is_empty() { state.summarizer_selected_columns.push(None); }
                    let mut summ_remove: Vec<usize> = Vec::new();
                    for i in 0..state.summarizer_selected_columns.len() {
                        ui.horizontal(|ui_h| {
                            let cur = state.summarizer_selected_columns[i];
                            let cur_label = cur.and_then(|ci| headers.get(ci).cloned()).unwrap_or_else(|| "(none)".to_string());
                            // use button+popup picker
                            let combo_id = format!("rp_summ_{}_combo_{}_{}", i, popup_sheet_name, state.options_column_target_index);
                            let filter_key = format!("{}_filter", combo_id);
                            let btn = ui_h.button(cur_label.clone());
                            let popup_id = egui::Id::new(combo_id.clone());
                            if btn.clicked() { ui_h.ctx().memory_mut(|mem| mem.open_popup(popup_id)); }
                            egui::containers::popup::popup_below_widget(
                                ui_h,
                                popup_id,
                                &btn,
                                egui::containers::popup::PopupCloseBehavior::CloseOnClickOutside,
                                |popup_ui| {
                                    let mut ftext = popup_ui.memory(|mem| mem.data.get_temp::<String>(filter_key.clone().into()).unwrap_or_default());
                                    let char_w = 8.0_f32;
                                    let max_name_len = headers.iter().map(|h| h.len()).max().unwrap_or(12);
                                    let mut popup_min_width = (max_name_len as f32) * char_w + 24.0;
                                    popup_min_width = popup_min_width.clamp(120.0, 900.0);
                                    popup_ui.set_min_width(popup_min_width);
                                    popup_ui.horizontal(|ui_h2| {
                                        ui_h2.label("Filter:");
                                        let avail = ui_h2.available_width();
                                        let default_chars = 28usize;
                                        let desired = (default_chars as f32) * char_w;
                                        let width = desired.min(avail).min(popup_min_width - 40.0);
                                        let resp = ui_h2.add(egui::TextEdit::singleline(&mut ftext).desired_width(width).hint_text("type to filter columns"));
                                        if resp.changed() { ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), ftext.clone())); }
                                        if ui_h2.small_button("x").clicked() { ftext.clear(); ui_h2.memory_mut(|mem| mem.data.insert_temp(filter_key.clone().into(), ftext.clone())); }
                                    });
                                    let current = normalize_for_link_cmp(&ftext);
                                    egui::ScrollArea::vertical().max_height(300.0).show(popup_ui, |list_ui| {
                                        if list_ui.selectable_label(cur.is_none(), "(none)").clicked() { state.summarizer_selected_columns[i] = None; list_ui.memory_mut(|mem| mem.close_popup()); }
                                        for (idx, h) in headers.iter().enumerate() {
                                            if !current.is_empty() && !normalize_for_link_cmp(h).contains(&current) { continue; }
                                            if list_ui.selectable_label(cur == Some(idx), h).clicked() { state.summarizer_selected_columns[i] = Some(idx); list_ui.memory_mut(|mem| mem.close_popup()); }
                                        }
                                    });
                                },
                            );
                            if ui_h.small_button("x").on_hover_text("Remove").clicked() { summ_remove.push(i); }
                        });
                    }
                    for idx in summ_remove.iter().rev() { if *idx < state.summarizer_selected_columns.len() { state.summarizer_selected_columns.remove(*idx); } }
                    {
                        let mut i = 0usize;
                        while i + 1 < state.summarizer_selected_columns.len() {
                            if state.summarizer_selected_columns[i].is_none() { state.summarizer_selected_columns.remove(i); } else { i += 1; }
                        }
                    }
                    if !state.summarizer_selected_columns.last().map(|o| o.is_none()).unwrap_or(false) { state.summarizer_selected_columns.push(None); }
                }
            }

            // Re-introduced separator above buttons per updated request
            ui.separator();
            ui.horizontal(|ui_h| {
                if ui_h.button("Apply").clicked() { apply_clicked = true; }
                if ui_h.button("Cancel").clicked() { cancel_clicked = true; }
            });
        });

    let close_via_x = state.show_random_picker_panel && !popup_open;
    RandomPickerUiResult {
        apply_clicked,
        cancel_clicked,
        close_via_x,
    }
}
