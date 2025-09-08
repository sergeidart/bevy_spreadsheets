fn cancel_batch(state: &mut EditorWindowState) {
    state.ai_batch_review_active = false;
    state.ai_mode = super::state::AiModeState::Idle;
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    state.ai_selected_rows.clear();
    // Also reset broader interaction modes and selections so the UI returns to normal (hides "Exit AI").
    state.reset_interaction_modes_and_selections();
}
// src/ui/elements/editor/ai_batch_review_ui.rs
use bevy::prelude::EventWriter;
// Batch AI review UI (refactored to use snapshot RowReview/NewRowReview model)
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, RichText, Align};
use egui_extras::{TableBuilder, Column};
use crate::sheets::{events::{UpdateCellEvent, AddSheetRowRequest}, resources::SheetRegistry};
use super::state::{EditorWindowState, ReviewChoice, ThrottledAiAction};

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

    // Resolve active sheet (respect virtual structure stack). Use owned String to avoid long-lived borrows of `state`.
    let active_sheet_name: String = if let Some(vctx) = state.virtual_structure_stack.last() { vctx.virtual_sheet_name.clone() } else if let Some(s) = selected_sheet_name_clone { s.clone() } else { return; };
    let sheet_opt = registry.get_sheet(selected_category_clone, &active_sheet_name);
    let metadata = match sheet_opt.and_then(|s| s.metadata.clone()) { Some(m) => m, None => { ui.colored_label(Color32::RED, "Metadata missing"); return; } };

    // Gather ancestor key columns using virtual_structure_stack logic from editor_sheet_display.rs
    let mut ancestor_key_columns: Vec<(String, String)> = Vec::new();
    if let Some(last_ctx) = state.virtual_structure_stack.last() {
        if last_ctx.virtual_sheet_name == active_sheet_name {
            for vctx in &state.virtual_structure_stack {
                if let Some(parent_sheet) = registry.get_sheet(selected_category_clone, &vctx.parent.parent_sheet) {
                    if let (Some(parent_meta), Some(parent_row)) = (&parent_sheet.metadata, parent_sheet.grid.get(vctx.parent.parent_row)) {
                        if let Some(struct_col_def) = parent_meta.columns.get(vctx.parent.parent_col) {
                            if let Some(key_col_idx) = struct_col_def.structure_key_parent_column_index {
                                if let Some(key_col_def) = parent_meta.columns.get(key_col_idx) {
                                    let value = parent_row.get(key_col_idx).cloned().unwrap_or_default();
                                    ancestor_key_columns.push((key_col_def.header.clone(), value));
                                }
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
    // No separate "End Review" button; use Decline All to cancel
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
        union_cols.sort_unstable();
        union_cols.dedup();

        builder = builder.column(Column::initial(CONTROL_COL_INITIAL).at_least(110.0).resizable(true).clip(true));
        // Add key columns only for AI textbox row (will be empty for other rows)
        for _ in &ancestor_key_columns { builder = builder.column(Column::initial(120.0).at_least(60.0).resizable(true).clip(true)); }
        for _ in &union_cols { builder = builder.column(Column::initial(120.0).at_least(60.0).resizable(true).clip(true)); }

        builder.header(22.0, |mut header| {
            header.col(|_ui| {});
            // Key column headers only for AI textbox row
            for (key_header, _) in &ancestor_key_columns { header.col(|ui| { ui.strong(key_header); }); }
            for col_idx in &union_cols { header.col(|ui| { let htxt = metadata.columns.get(*col_idx).map(|c| c.header.as_str()).unwrap_or(""); ui.strong(htxt); }); }
        }).body(|mut body| {
            for blk in &blocks {
                body.row(22.0, |mut row| {
                    match blk {
                        RowBlock::ExistingOrig(idx) => {
                            if let Some(rr) = state.ai_row_reviews.get(*idx) {
                                // Row 1: Accept | (nothing) | Original content (possibly struck-through)
                                row.col(|ui| {
                                    if ui.button("Accept").clicked() { existing_accept.push(*idx); }
                                });
                                // Key columns are empty on the original row
                                for _ in &ancestor_key_columns { row.col(|ui| { ui.add_space(0.0); }); }
                                for actual_col in &union_cols {
                                    row.col(|ui| {
                                        let pos_in_row = rr.non_structure_columns.iter().position(|c| c == actual_col);
                                        let orig_val = pos_in_row.and_then(|p| rr.original.get(p)).cloned().unwrap_or_default();
                                        let ai_val = pos_in_row.and_then(|p| rr.ai.get(p)).cloned().unwrap_or_default();
                                        let choice = pos_in_row.and_then(|p| rr.choices.get(p)).cloned().unwrap_or(ReviewChoice::Original);
                                        let strike = orig_val != ai_val && matches!(choice, ReviewChoice::AI);
                                        let text = if strike { RichText::new(orig_val).strikethrough() } else { RichText::new(orig_val) };
                                        ui.label(text);
                                    });
                                }
                            }
                        }
                        RowBlock::ExistingAi(idx) => {
                            if let Some(rr) = state.ai_row_reviews.get_mut(*idx) {
                                // Row 2: Cancel | Key columns (values) | AI suggested editable values
                                row.col(|ui| {
                                    if ui.button("Cancel").clicked() { existing_cancel.push(*idx); }
                                });
                                for (_, value) in &ancestor_key_columns {
                                    row.col(|ui| { ui.label(RichText::new(value).color(Color32::LIGHT_GREEN)); });
                                }
                                for actual_col in &union_cols {
                                    row.col(|ui| {
                                        if let Some(pos) = rr.non_structure_columns.iter().position(|c| c == actual_col) {
                                            if let Some(cell) = rr.ai.get_mut(pos) {
                                                let orig_val = rr.original.get(pos).cloned().unwrap_or_default();
                                                let is_diff = *cell != orig_val;
                                                ui.add(egui::TextEdit::singleline(cell).desired_width(f32::INFINITY).text_color_opt(if is_diff { Some(Color32::LIGHT_YELLOW) } else { None }));
                                            } else {
                                                ui.label("");
                                            }
                                        } else {
                                            ui.label("");
                                        }
                                    });
                                }
                            }
                        }
                        RowBlock::ExistingChoices(idx) => {
                            if let Some(rr) = state.ai_row_reviews.get_mut(*idx) {
                                // Row 3: nothing | nothing | Orig/AI picker for original columns
                                row.col(|ui| { ui.add_space(2.0); });
                                // leave key columns empty on the choices row
                                for _ in &ancestor_key_columns { row.col(|ui| { ui.add_space(2.0); }); }
                                for actual_col in &union_cols {
                                    row.col(|ui| {
                                        if let Some(pos) = rr.non_structure_columns.iter().position(|c| c == actual_col) {
                                            let orig_val = rr.original.get(pos).cloned().unwrap_or_default();
                                            let ai_val = rr.ai.get(pos).cloned().unwrap_or_default();
                                            if orig_val == ai_val {
                                                ui.small(RichText::new("Same").color(Color32::GRAY));
                                            } else {
                                                let mut choice = rr.choices.get(pos).cloned().unwrap_or(ReviewChoice::Original);
                                                if ui.radio_value(&mut choice, ReviewChoice::Original, "Orig").clicked() {
                                                    if pos < rr.choices.len() { rr.choices[pos] = ReviewChoice::Original; }
                                                }
                                                if ui.radio_value(&mut choice, ReviewChoice::AI, "AI").clicked() {
                                                    if pos < rr.choices.len() { rr.choices[pos] = ReviewChoice::AI; }
                                                }
                                            }
                                        } else {
                                            ui.label("");
                                        }
                                    });
                                }
                            }
                        }
                        RowBlock::NewAi(idx) => {
                            if let Some(nr) = state.ai_new_row_reviews.get_mut(*idx) {
                                // New row - Row 1: Accept | (nothing) | "AI Added" title
                                row.col(|ui| {
                                    if ui.button("Accept").clicked() { new_accept.push(*idx); }
                                });
                                for _ in &ancestor_key_columns { row.col(|ui| { ui.add_space(0.0); }); }
                                for (i, actual_col) in union_cols.iter().enumerate() {
                                    row.col(|ui| {
                                        if i == 0 {
                                            ui.colored_label(Color32::LIGHT_BLUE, "AI Added");
                                        } else {
                                            ui.label("");
                                        }
                                    });
                                }
                            }
                        }
                        RowBlock::NewMarker(idx) => {
                            if let Some(nr) = state.ai_new_row_reviews.get_mut(*idx) {
                                // New row - Row 2: Cancel | Key Column Cell(s) | AI suggested values
                                row.col(|ui| {
                                    if ui.button("Cancel").clicked() { new_cancel.push(*idx); }
                                });
                                for (_, value) in &ancestor_key_columns {
                                    row.col(|ui| { ui.label(RichText::new(value).color(Color32::LIGHT_GREEN)); });
                                }
                                for actual_col in &union_cols {
                                    row.col(|ui| {
                                        if let Some(pos) = nr.non_structure_columns.iter().position(|c| c == actual_col) {
                                            if let Some(cell) = nr.ai.get_mut(pos) { ui.add(egui::TextEdit::singleline(cell).desired_width(f32::INFINITY)); } else { ui.label(""); }
                                        } else { ui.label(""); }
                                    });
                                }
                            }
                        }
                    }
                });
            }
        });
    }); // <-- Add this to close the ScrollArea closure

    // Handle header actions (Accept All / Decline All)
    if accept_all_clicked_header {
        // Deterministic, per-row application:
        // 1) Apply all existing row changes in order, honoring per-column choices.
    let existing_len = state.ai_row_reviews.len();
        for idx in 0..existing_len {
            if let Some(rr) = state.ai_row_reviews.get(idx) {
                for (pos, actual_col) in rr.non_structure_columns.iter().enumerate() {
                    let choice = rr.choices.get(pos).cloned().unwrap_or(ReviewChoice::Original);
                    if matches!(choice, ReviewChoice::AI) {
                        if let Some(ai_val) = rr.ai.get(pos).cloned() {
                            info!("AcceptAll(batch-existing): row={} col={} val={}", rr.row_index, *actual_col, ai_val);
                            state.ai_throttled_apply_queue.push_back(ThrottledAiAction::UpdateCell { row_index: rr.row_index, col_index: *actual_col, value: ai_val });
                        }
                    }
                }
            }
        }
        // 2) Apply all new rows in order returned.
    let new_len = state.ai_new_row_reviews.len();
        for idx in 0..new_len {
            if let Some(nr) = state.ai_new_row_reviews.get(idx) {
                let mut init_vals: Vec<(usize, String)> = Vec::new();
                for (pos, actual_col) in nr.non_structure_columns.iter().enumerate() {
                    if let Some(val) = nr.ai.get(pos).cloned() { init_vals.push((*actual_col, val)); }
                }
                info!("AcceptAll(batch-new): cols={} (first few shown)", init_vals.len());
                state.ai_throttled_apply_queue.push_back(ThrottledAiAction::AddRow { initial_values: init_vals });
            }
        }
        // 3) Clear and exit batch review
        cancel_batch(state);
        // Early return to avoid double-processing below
        return;
    }
    if decline_all_clicked_header {
        for i in 0..state.ai_row_reviews.len() { existing_cancel.push(i); }
        for i in 0..state.ai_new_row_reviews.len() { new_cancel.push(i); }
    }

    // Normalize and process existing accepts/cancels (remove from highest index down)
    existing_accept.sort_unstable(); existing_accept.dedup();
    existing_cancel.sort_unstable(); existing_cancel.dedup();
    // To avoid double-processing an index present in both accept and cancel, prefer accept.
    existing_cancel.retain(|i| !existing_accept.contains(i));

    // Process accepts: apply per-column according to rr.choices
    if !existing_accept.is_empty() {
        // iterate in reverse so removing by index won't shift earlier indices
        existing_accept.sort_unstable_by(|a,b| b.cmp(a));
        for idx in existing_accept.iter() {
            if let Some(rr) = state.ai_row_reviews.get(*idx) {
                for (pos, actual_col) in rr.non_structure_columns.iter().enumerate() {
                    let choice = rr.choices.get(pos).cloned().unwrap_or(ReviewChoice::Original);
                    if matches!(choice, ReviewChoice::AI) {
                        if let Some(ai_val) = rr.ai.get(pos).cloned() {
                            cell_update_writer.write(UpdateCellEvent { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), row_index: rr.row_index, col_index: *actual_col, new_value: ai_val });
                        }
                    }
                }
            }
            // remove processed review
            if *idx < state.ai_row_reviews.len() { state.ai_row_reviews.remove(*idx); }
        }
    }

    if !existing_cancel.is_empty() {
        existing_cancel.sort_unstable_by(|a,b| b.cmp(a));
        for idx in existing_cancel.iter() {
            if *idx < state.ai_row_reviews.len() { state.ai_row_reviews.remove(*idx); }
        }
    }

    // Process new rows acceptance / cancellation
    new_accept.sort_unstable(); new_accept.dedup();
    new_cancel.sort_unstable(); new_cancel.dedup();
    new_cancel.retain(|i| !new_accept.contains(i));

    if !new_accept.is_empty() {
        new_accept.sort_unstable_by(|a,b| b.cmp(a));
        for idx in new_accept.iter() {
            if let Some(nr) = state.ai_new_row_reviews.get(*idx) {
                // Build initial values mapping actual col -> value
                let mut init_vals: Vec<(usize, String)> = Vec::new();
                for (pos, actual_col) in nr.non_structure_columns.iter().enumerate() {
                    if let Some(val) = nr.ai.get(pos).cloned() {
                        init_vals.push((*actual_col, val));
                    }
                }
                let req = AddSheetRowRequest { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), initial_values: if init_vals.is_empty() { None } else { Some(init_vals) } };
                add_row_writer.write(req);
            }
            if *idx < state.ai_new_row_reviews.len() { state.ai_new_row_reviews.remove(*idx); }
        }
    }

    if !new_cancel.is_empty() {
        new_cancel.sort_unstable_by(|a,b| b.cmp(a));
        for idx in new_cancel.iter() {
            if *idx < state.ai_new_row_reviews.len() { state.ai_new_row_reviews.remove(*idx); }
        }
    }

    // Exit batch review if nothing left
    if state.ai_row_reviews.is_empty() && state.ai_new_row_reviews.is_empty() {
        cancel_batch(state);
    }
}
