// src/ui/elements/editor/ai_batch_review_ui.rs
use bevy::prelude::EventWriter;
// Batch AI review UI (refactored to use snapshot RowReview/NewRowReview model)
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, RichText, Align};
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

    // Auto-exit if nothing left
    if state.ai_row_reviews.is_empty() && state.ai_new_row_reviews.is_empty() {
        cancel_batch(state);
        return;
    }

    // Resolve active sheet (respect virtual structure stack)
    let active_sheet_name = if let Some(vctx) = state.virtual_structure_stack.last() { vctx.virtual_sheet_name.as_str() } else if let Some(s) = selected_sheet_name_clone { s.as_str() } else { return; };
    let sheet_opt = registry.get_sheet(selected_category_clone, active_sheet_name);
    let metadata = match sheet_opt.and_then(|s| s.metadata.clone()) { Some(m) => m, None => { ui.colored_label(Color32::RED, "Metadata missing"); return; } };

    // Derive key header/value from deepest virtual structure context (if any)
    let mut key_header: Option<String> = None;
    let mut key_value: Option<String> = None;
    if !state.virtual_structure_stack.is_empty() {
        for vctx in &state.virtual_structure_stack {
            if let Some(parent_sheet) = registry.get_sheet(&state.selected_category, &vctx.parent.parent_sheet) {
                if let (Some(parent_meta), Some(parent_row)) = (&parent_sheet.metadata, parent_sheet.grid.get(vctx.parent.parent_row)) {
                    if let Some(struct_col_def) = parent_meta.columns.get(vctx.parent.parent_col) {
                        if let Some(key_col_idx) = struct_col_def.structure_key_parent_column_index {
                            if let Some(key_col_def) = parent_meta.columns.get(key_col_idx) {
                                key_header = Some(key_col_def.header.clone());
                                key_value = Some(parent_row.get(key_col_idx).cloned().unwrap_or_default());
                            }
                        }
                    }
                }
            }
        }
    }

    // Header actions
    let mut accept_all_clicked_header = false;
    let mut decline_all_clicked_header = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("AI Review").heading());
        ui.add_space(12.0);
        if ui.button(RichText::new("Accept All").strong()).clicked() { accept_all_clicked_header = true; }
        if ui.button(RichText::new("Decline All").color(Color32::LIGHT_RED)).clicked() { decline_all_clicked_header = true; }
    });

    // Table construction using snapshots.
    // We render blocks of 3 rows per existing original (Original / AI / Choices) and 2 rows per new row (AI / Marker).
    enum RowBlock { ExistingOrig(usize), ExistingAi(usize), ExistingChoices(usize), NewAi(usize), NewMarker(usize) }
    let mut blocks: Vec<RowBlock> = Vec::new();
    for (i, _rr) in state.ai_row_reviews.iter().enumerate() {
        blocks.push(RowBlock::ExistingOrig(i));
        blocks.push(RowBlock::ExistingAi(i));
        blocks.push(RowBlock::ExistingChoices(i));
    }
    for (i, _nr) in state.ai_new_row_reviews.iter().enumerate() {
        blocks.push(RowBlock::NewAi(i));
        blocks.push(RowBlock::NewMarker(i));
    }

    // Pending actions collected during UI pass
    let mut existing_accept: Vec<usize> = Vec::new();
    let mut existing_cancel: Vec<usize> = Vec::new();
    let mut new_accept: Vec<usize> = Vec::new();
    let mut new_cancel: Vec<usize> = Vec::new();
    let mut pending_updates: Vec<(usize, usize, String)> = Vec::new(); // (row_index, col_index, new_value)

    const CONTROL_COL_INITIAL: f32 = 160.0;
    egui::ScrollArea::both().id_salt("ai_batch_review_table_scroll_snapshots").auto_shrink([false,false]).show(ui, |ui| {
        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(Align::Center))
            .min_scrolled_height(0.0);

        // Determine a canonical non-structure column ordering union across rows (they should match, but be defensive)
        let mut union_cols: Vec<usize> = state.ai_row_reviews.first().map(|r| r.non_structure_columns.clone()).unwrap_or_else(|| state.ai_new_row_reviews.first().map(|r| r.non_structure_columns.clone()).unwrap_or_default());
        // Ensure uniqueness & sorted (in case of anomalies)
        union_cols.sort_unstable();
        union_cols.dedup();

        builder = builder.column(Column::initial(CONTROL_COL_INITIAL).at_least(110.0).resizable(true).clip(true));
        if key_header.is_some() { builder = builder.column(Column::initial(120.0).at_least(60.0).resizable(true).clip(true)); }
        for _ in &union_cols { builder = builder.column(Column::initial(120.0).at_least(60.0).resizable(true).clip(true)); }

        builder.header(22.0, |mut header| {
            header.col(|_ui| {});
            if let Some(h) = key_header.as_ref() { header.col(|ui| { ui.colored_label(Color32::from_rgb(0,170,0), RichText::new(h).strong()); }); }
            for col_idx in &union_cols { header.col(|ui| { let htxt = metadata.columns.get(*col_idx).map(|c| c.header.as_str()).unwrap_or(""); ui.strong(htxt); }); }
        }).body(|mut body| {
            for blk in &blocks {
                body.row(24.0, |mut row| {
                    match blk {
                        RowBlock::ExistingOrig(idx) => {
                            if let Some(rr) = state.ai_row_reviews.get(*idx) {
                                row.col(|ui| { ui.horizontal(|ui| {
                                    if ui.button("Accept").clicked() { existing_accept.push(*idx); }
                                    if ui.button("Cancel").clicked() { existing_cancel.push(*idx); }
                                }); });
                                if key_header.is_some() { row.col(|ui| { let val = key_value.clone().unwrap_or_default(); ui.colored_label(Color32::from_rgb(0,150,0), val); }); }
                                for (j, actual_col) in union_cols.iter().enumerate() { row.col(|ui| {
                                    // Find mapping position in this row's subset
                                    let pos_in_row = rr.non_structure_columns.iter().position(|c| c == actual_col);
                                    let orig_val = pos_in_row.and_then(|p| rr.original.get(p)).cloned().unwrap_or_default();
                                    let ai_val = pos_in_row.and_then(|p| rr.ai.get(p)).cloned().unwrap_or_default();
                                    let choice = pos_in_row.and_then(|p| rr.choices.get(p)).cloned().unwrap_or(ReviewChoice::Original);
                                    let strike = orig_val != ai_val && matches!(choice, ReviewChoice::AI);
                                    let text = if strike { RichText::new(orig_val).strikethrough() } else { RichText::new(orig_val) };
                                    ui.label(text);
                                }); }
                            }
                        }
                        RowBlock::ExistingAi(idx) => {
                            if let Some(rr) = state.ai_row_reviews.get_mut(*idx) {
                                row.col(|ui| { ui.add_space(4.0); });
                                if key_header.is_some() { row.col(|ui| { ui.add_space(2.0); }); }
                                for actual_col in &union_cols { row.col(|ui| {
                                    if let Some(pos) = rr.non_structure_columns.iter().position(|c| c == actual_col) {
                                        if let Some(cell) = rr.ai.get_mut(pos) {
                                            let orig_val = rr.original.get(pos).cloned().unwrap_or_default();
                                            let is_diff = *cell != orig_val;
                                            ui.add(egui::TextEdit::singleline(cell).desired_width(f32::INFINITY).text_color_opt(if is_diff { Some(Color32::LIGHT_YELLOW) } else { None }));
                                        } else { ui.label(""); }
                                    } else { ui.label(""); }
                                }); }
                            }
                        }
                        RowBlock::ExistingChoices(idx) => {
                            if let Some(rr) = state.ai_row_reviews.get_mut(*idx) {
                                row.col(|ui| { ui.add_space(2.0); });
                                if key_header.is_some() { row.col(|ui| { ui.add_space(2.0); }); }
                                for actual_col in &union_cols { row.col(|ui| {
                                    if let Some(pos) = rr.non_structure_columns.iter().position(|c| c == actual_col) {
                                        let orig_val = rr.original.get(pos).cloned().unwrap_or_default();
                                        let ai_val = rr.ai.get(pos).cloned().unwrap_or_default();
                                        if orig_val == ai_val { ui.small(RichText::new("Same").color(Color32::GRAY)); } else {
                                            let mut choice = rr.choices.get(pos).cloned().unwrap_or(ReviewChoice::Original);
                                            if ui.radio_value(&mut choice, ReviewChoice::Original, "Orig").clicked() { if pos < rr.choices.len() { rr.choices[pos] = ReviewChoice::Original; } }
                                            if ui.radio_value(&mut choice, ReviewChoice::AI, "AI").clicked() { if pos < rr.choices.len() { rr.choices[pos] = ReviewChoice::AI; } }
                                        }
                                    } else { ui.label(""); }
                                }); }
                            }
                        }
                        RowBlock::NewAi(idx) => {
                            if let Some(nr) = state.ai_new_row_reviews.get_mut(*idx) {
                                row.col(|ui| { ui.horizontal(|ui| {
                                    if ui.button("Accept").clicked() { new_accept.push(*idx); }
                                    if ui.button("Cancel").clicked() { new_cancel.push(*idx); }
                                    ui.colored_label(Color32::LIGHT_BLUE, "AI");
                                }); });
                                if key_header.is_some() { row.col(|ui| { ui.add_space(2.0); }); }
                                for actual_col in &union_cols { row.col(|ui| {
                                    if let Some(pos) = nr.non_structure_columns.iter().position(|c| c == actual_col) {
                                        if let Some(cell) = nr.ai.get_mut(pos) { ui.add(egui::TextEdit::singleline(cell).desired_width(f32::INFINITY)); } else { ui.label(""); }
                                    } else { ui.label(""); }
                                }); }
                            }
                        }
                        RowBlock::NewMarker(_idx) => {
                            row.col(|ui| { ui.add_space(2.0); });
                            if key_header.is_some() { row.col(|ui| { ui.add_space(2.0); }); }
                            for _ in &union_cols { row.col(|ui| { ui.label(""); }); }
                        }
                    }
                });
            }
        });
    });

    // Bottom actions (repeat)
    ui.add_space(6.0);
    let mut accept_all_clicked = accept_all_clicked_header;
    let mut decline_all_clicked = decline_all_clicked_header;
    ui.horizontal(|ui| {
        if ui.button(RichText::new("Accept All").strong()).clicked() { accept_all_clicked = true; }
        if ui.button(RichText::new("Decline All").color(Color32::LIGHT_RED)).clicked() { decline_all_clicked = true; }
    });

    if decline_all_clicked { cancel_batch(state); return; }

    // Helper closure to apply a single existing row review
    let mut apply_existing_row = |rr: &super::state::RowReview| {
        for (pos, actual_col) in rr.non_structure_columns.iter().enumerate() {
            let choice = rr.choices.get(pos).cloned().unwrap_or(ReviewChoice::Original);
            let orig_val = rr.original.get(pos).cloned().unwrap_or_default();
            let ai_val = rr.ai.get(pos).cloned().unwrap_or_default();
            let final_val = match choice { ReviewChoice::Original => orig_val.clone(), ReviewChoice::AI => ai_val.clone() };
            if final_val != orig_val { pending_updates.push((rr.row_index, *actual_col, final_val)); }
        }
    };

    // Process individual accepts/cancels for existing rows
    if !existing_accept.is_empty() || !existing_cancel.is_empty() {
        existing_accept.sort_unstable(); existing_cancel.sort_unstable();
        // Apply accepts
        for idx in &existing_accept { if let Some(rr) = state.ai_row_reviews.get(*idx) { apply_existing_row(rr); } }
        // Remove rows (reverse for stability)
        for idx in existing_accept.into_iter().chain(existing_cancel.into_iter()).collect::<Vec<_>>().into_iter().rev() {
            if idx < state.ai_row_reviews.len() { state.ai_row_reviews.remove(idx); }
        }
    }

    // Process individual new row accepts/cancels
    if !new_accept.is_empty() || !new_cancel.is_empty() {
        new_accept.sort_unstable(); new_cancel.sort_unstable();
        // Apply accepts (reverse so user order preserved at top insertion)
        for idx in new_accept.iter().rev() { if let Some(nr) = state.ai_new_row_reviews.get(*idx) {
            // Build initial values payload for insertion time
            let mut init_vals: Vec<(usize, String)> = Vec::new();
            for (pos, actual_col) in nr.non_structure_columns.iter().enumerate() {
                if let Some(val) = nr.ai.get(pos) { if !val.is_empty() { init_vals.push((*actual_col, val.clone())); } }
            }
            // Also clear structure columns explicitly at creation
            for (i, c) in metadata.columns.iter().enumerate() { if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { init_vals.push((i, String::new())); } }
            add_row_writer.write(AddSheetRowRequest { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), initial_values: Some(init_vals) });
        }}
        for idx in new_accept.into_iter().chain(new_cancel.into_iter()).collect::<Vec<_>>().into_iter().rev() { if idx < state.ai_new_row_reviews.len() { state.ai_new_row_reviews.remove(idx); } }
    }

    // Apply Accept All
    if accept_all_clicked {
        // Existing rows
        for rr in &state.ai_row_reviews { apply_existing_row(rr); }
        // New rows (reverse insertion order to keep first near top)
        for nr in state.ai_new_row_reviews.iter().rev() {
            let mut init_vals: Vec<(usize, String)> = Vec::new();
            for (pos, actual_col) in nr.non_structure_columns.iter().enumerate() { if let Some(val) = nr.ai.get(pos) { if !val.is_empty() { init_vals.push((*actual_col, val.clone())); } } }
            for (i, c) in metadata.columns.iter().enumerate() { if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { init_vals.push((i, String::new())); } }
            add_row_writer.write(AddSheetRowRequest { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), initial_values: Some(init_vals) });
        }
        state.ai_row_reviews.clear();
        state.ai_new_row_reviews.clear();
    }

    // Emit updates for existing rows now
    for (row_index, col_index, new_value) in pending_updates.drain(..) {
        cell_update_writer.write(UpdateCellEvent { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), row_index, col_index, new_value });
    }

    // Auto-exit if emptied by actions
    if state.ai_row_reviews.is_empty() && state.ai_new_row_reviews.is_empty() { cancel_batch(state); }
}

fn cancel_batch(state: &mut EditorWindowState) {
    state.ai_batch_review_active = false;
    state.ai_mode = super::state::AiModeState::Idle;
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    state.ai_selected_rows.clear();
}
