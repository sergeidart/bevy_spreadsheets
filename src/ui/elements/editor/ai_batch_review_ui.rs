// src/ui/elements/editor/ai_batch_review_ui.rs
use bevy::prelude::EventWriter;
// Clean implementation of batch AI review UI (multi-row, per-column choices)
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, RichText};
use egui_extras::{TableBuilder, Column};
use crate::sheets::{events::{UpdateCellEvent, AddSheetRowRequest}, resources::SheetRegistry};
use super::state::{EditorWindowState, ReviewChoice};

pub(super) fn draw_ai_batch_review_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    registry: &SheetRegistry,
    cell_update_writer: &mut EventWriter<UpdateCellEvent>,
    add_row_writer: &mut EventWriter<AddSheetRowRequest>,
) {
    if !state.ai_batch_review_active { return; }
    let active_sheet_name = if let Some(vctx) = state.virtual_structure_stack.last() { vctx.virtual_sheet_name.as_str() } else if let Some(s) = selected_sheet_name_clone { s.as_str() } else { return; };
    let mut row_indices: Vec<usize> = state.ai_batch_suggestion_buffers.keys().cloned().collect();
    row_indices.sort_unstable();
    if row_indices.is_empty() { ui.colored_label(Color32::YELLOW, "No batch suggestions."); return; }
    let sheet_opt = registry.get_sheet(selected_category_clone, active_sheet_name);
    let metadata = match sheet_opt.and_then(|s| s.metadata.clone()) { Some(m) => m, None => { ui.colored_label(Color32::RED, "Metadata missing"); return; } };
    let visible_cols: Vec<usize> = metadata.columns.iter().enumerate()
        .filter_map(|(i,c)| if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { None } else { Some(i) })
        .collect();

    ui.label(RichText::new(format!("Batch AI Review ({} rows)", row_indices.len())).heading());

    let mut apply_all_clicked = false; // handled after scroll
    let mut cancel_all_clicked = false;

    const CONTROL_COL_WIDTH: f32 = 110.0; // fixed width for left per-row control stack

    // Collect diffs after UI to avoid borrow conflicts
    let mut pending_cell_updates: Vec<(usize, usize, String)> = Vec::new();
    let mut rows_to_drop: Vec<usize> = Vec::new();

    egui::ScrollArea::horizontal().id_salt("ai_batch_hscroll").show(ui, |ui| {
        ui.vertical(|ui| {
            // Header row
            ui.horizontal(|ui| {
                ui.add_sized([CONTROL_COL_WIDTH, 20.0], egui::Label::new(""));
                for col_index in &visible_cols {
                    let header = metadata.columns.get(*col_index).map(|c| c.header.as_str()).unwrap_or("");
                    ui.add_sized([80.0, 20.0], egui::Label::new(RichText::new(header).strong()));
                }
            });
            ui.add_space(4.0);
            // Rows scroll
            egui::ScrollArea::vertical().id_salt("ai_batch_rows_scroll").show(ui, |ui| {
                for row_index in row_indices.iter().cloned() {
                    let original_row_opt = sheet_opt.and_then(|s| s.grid.get(row_index)).cloned();
                    if original_row_opt.is_none() { continue; }
                    let original_row = original_row_opt.unwrap();
                    let choices_entry = state.ai_batch_column_choices.entry(row_index).or_insert_with(|| vec![ReviewChoice::AI; visible_cols.len()]);
                    if choices_entry.len() != visible_cols.len() { *choices_entry = vec![ReviewChoice::AI; visible_cols.len()]; }
                    let suggestion_entry = match state.ai_batch_suggestion_buffers.get_mut(&row_index) { Some(s) => s, None => continue };
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        let mut apply_row = false;
                        let mut skip_row = false;
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(egui::Vec2::new(CONTROL_COL_WIDTH, 70.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.label(RichText::new(format!("Row {}", row_index)).strong());
                                if ui.button("Apply Row").clicked() { apply_row = true; }
                                if ui.button("Cancel").clicked() { skip_row = true; }
                            });
                            let row_height = 22.0;
                            let table_id = format!("batch_row_table_{}", row_index);
                            TableBuilder::new(ui)
                                .id_salt(table_id)
                                .striped(true)
                                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                .columns(Column::auto().at_least(80.0), visible_cols.len())
                                .body(|mut body| {
                                    body.row(row_height, |mut r| {
                                        for (display_idx, actual_col) in visible_cols.iter().enumerate() {
                                            r.col(|ui| {
                                                let orig_val = original_row.get(*actual_col).cloned().unwrap_or_default();
                                                let ai_val = suggestion_entry.get(*actual_col).cloned().unwrap_or_default();
                                                let is_diff = orig_val != ai_val && matches!(choices_entry.get(display_idx), Some(ReviewChoice::AI));
                                                let text = if is_diff { RichText::new(orig_val).strikethrough() } else { RichText::new(orig_val) };
                                                ui.label(text);
                                            });
                                        }
                                    });
                                    body.row(row_height, |mut r| {
                                        for (_display_idx, actual_col) in visible_cols.iter().enumerate() {
                                            r.col(|ui| {
                                                if let Some(cell) = suggestion_entry.get_mut(*actual_col) {
                                                    let orig_val = original_row.get(*actual_col).cloned().unwrap_or_default();
                                                    let is_empty = cell.trim().is_empty();
                                                    let is_diff = !is_empty && *cell != orig_val;
                                                    let resp = ui.add(egui::TextEdit::singleline(cell).desired_width(f32::INFINITY));
                                                    if resp.hovered() || resp.has_focus() { /* keep underline visible anyway */ }
                                                    if is_empty || is_diff {
                                                        let color = if is_empty { Color32::from_rgb(180, 20, 20) } else { Color32::from_rgb(210, 120, 0) }; // dark red / dark orange
                                                        let stroke = egui::Stroke{ width: 2.0, color };
                                                        let y = resp.rect.max.y - 1.0;
                                                        ui.painter().hline(resp.rect.x_range(), y, stroke);
                                                    }
                                                }
                                            });
                                        }
                                    });
                                    body.row(row_height, |mut r| {
                                        for display_idx in 0..visible_cols.len() {
                                            r.col(|ui| {
                                                let actual_col = visible_cols[display_idx];
                                                let orig_val = original_row.get(actual_col).cloned().unwrap_or_default();
                                                let ai_val = suggestion_entry.get(actual_col).cloned().unwrap_or_default();
                                                if orig_val == ai_val { ui.small(RichText::new("Same").color(Color32::GRAY)); } else {
                                                    let mut choice = choices_entry[display_idx];
                                                    if ui.radio_value(&mut choice, ReviewChoice::Original, "Orig").clicked() { choices_entry[display_idx] = ReviewChoice::Original; }
                                                    if ui.radio_value(&mut choice, ReviewChoice::AI, "AI").clicked() { choices_entry[display_idx] = ReviewChoice::AI; }
                                                }
                                            });
                                        }
                                    });
                                });
                        });
                        if skip_row { rows_to_drop.push(row_index); }
                        if let Some(prefix) = state.ai_context_prefix_by_row.get(&row_index) { if !prefix.is_empty() {
                            ui.colored_label(Color32::LIGHT_BLUE, "Context:");
                            egui::Grid::new(format!("ctx_grid_{}", row_index)).num_columns(2).show(ui, |g| {
                                for (h,v) in prefix { g.label(RichText::new(h).strong()); g.label(v); g.end_row(); }
                            });
                        }}
                        if apply_row {
                            for (display_idx, actual_col) in visible_cols.iter().enumerate() {
                                let orig_val = original_row.get(*actual_col).cloned().unwrap_or_default();
                                let ai_val = suggestion_entry.get(*actual_col).cloned().unwrap_or_default();
                                let choice = choices_entry.get(display_idx).unwrap_or(&ReviewChoice::Original);
                                let final_val = match choice { ReviewChoice::Original => &orig_val, ReviewChoice::AI => &ai_val };
                                if final_val != &orig_val { pending_cell_updates.push((row_index, *actual_col, final_val.clone())); }
                            }
                            rows_to_drop.push(row_index);
                        }
                    });
                    ui.add_space(6.0);
                }
                if !state.ai_staged_new_rows.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(format!("Proposed New Rows ({}):", state.ai_staged_new_rows.len())).strong());
                    for (i, row_vals) in state.ai_staged_new_rows.iter_mut().enumerate() {
                        let mut accept = state.ai_staged_new_row_accept.get(i).cloned().unwrap_or(true);
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut accept, format!("Accept New Row #{}", i+1));
                                if ui.button("Drop").clicked() { accept = false; }
                            });
                            if i < state.ai_staged_new_row_accept.len() { state.ai_staged_new_row_accept[i] = accept; }
                            egui::Grid::new(format!("new_row_grid_{}", i)).num_columns(2).show(ui, |g| {
                                for col_index in &visible_cols {
                                    let header = metadata.columns.get(*col_index).map(|c| c.header.as_str()).unwrap_or("");
                                    let cell_ref = if let Some(cell) = row_vals.get_mut(*col_index) { cell } else { continue };
                                    g.label(RichText::new(header).strong());
                                    g.add(egui::TextEdit::singleline(cell_ref).desired_width(180.0));
                                    g.end_row();
                                }
                            });
                        });
                        ui.add_space(6.0);
                    }
                }
            }); // end rows vertical scroll
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_sized([CONTROL_COL_WIDTH, 0.0], egui::Label::new(""));
                if ui.button(RichText::new("Apply All").strong()).clicked() { apply_all_clicked = true; }
                ui.add_space(24.0);
                if ui.button("Cancel").clicked() { cancel_all_clicked = true; }
            });
        }); // end vertical
    }); // end horizontal scroll

    if cancel_all_clicked { cancel_batch(state); return; }

    // Emit events after UI borrows ended (row-level applies)
    for (row, col, new_val) in pending_cell_updates { cell_update_writer.write(UpdateCellEvent { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), row_index: row, col_index: col, new_value: new_val }); }
    for r in rows_to_drop { state.ai_batch_suggestion_buffers.remove(&r); state.ai_batch_column_choices.remove(&r); }

    if apply_all_clicked {
        // Apply all remaining rows
        for row_index in state.ai_batch_suggestion_buffers.keys().cloned().collect::<Vec<_>>() {
            if let Some(original_row) = sheet_opt.and_then(|s| s.grid.get(row_index)).cloned() {
                if let Some(suggestion_entry) = state.ai_batch_suggestion_buffers.get(&row_index) {
                    let choices_entry = state.ai_batch_column_choices.entry(row_index).or_insert_with(|| vec![ReviewChoice::AI; visible_cols.len()]);
                    for (display_idx, actual_col) in visible_cols.iter().enumerate() {
                        let orig_val = original_row.get(*actual_col).cloned().unwrap_or_default();
                        let ai_val = suggestion_entry.get(*actual_col).cloned().unwrap_or_default();
                        let choice = choices_entry.get(display_idx).unwrap_or(&ReviewChoice::Original);
                        let final_val = match choice { ReviewChoice::Original => &orig_val, ReviewChoice::AI => &ai_val };
                        if final_val != &orig_val { cell_update_writer.write(UpdateCellEvent { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), row_index, col_index: *actual_col, new_value: final_val.clone() }); }
                    }
                }
            }
        }
        // Handle accepted new rows insertion if any
        let accepted: Vec<Vec<String>> = state.ai_staged_new_rows.iter().zip(state.ai_staged_new_row_accept.iter())
            .filter_map(|(row, acc)| if *acc { Some(row.clone()) } else { None })
            .collect();
        if !accepted.is_empty() {
            for _row_vals in accepted.iter().rev() { add_row_writer.write(AddSheetRowRequest { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string() }); }
            for (i, raw_row) in accepted.iter().enumerate() {
                if state.ai_included_non_structure_columns.is_empty() { continue; }
                let max_col = *state.ai_included_non_structure_columns.iter().max().unwrap_or(&0);
                let mut expanded = vec![String::new(); max_col + 1];
                for (j, actual_col) in state.ai_included_non_structure_columns.iter().enumerate() {
                    if let Some(val) = raw_row.get(j) { if let Some(slot) = expanded.get_mut(*actual_col) { *slot = val.clone(); } }
                }
                for (col_index, val) in expanded.into_iter().enumerate() { if !val.is_empty() { cell_update_writer.write(UpdateCellEvent { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), row_index: i, col_index, new_value: val }); } }
            }
        }
        cancel_batch(state);
    }
}

fn cancel_batch(state: &mut EditorWindowState) {
    state.ai_batch_review_active = false;
    state.ai_mode = super::state::AiModeState::Idle;
    state.ai_batch_suggestion_buffers.clear();
    state.ai_batch_column_choices.clear();
    state.ai_suggestions.clear();
    state.ai_selected_rows.clear();
}
