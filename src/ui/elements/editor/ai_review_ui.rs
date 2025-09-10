// src/ui/elements/editor/ai_review_ui.rs
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, RichText, TextStyle, Align};
use egui_extras::{TableBuilder, Column}; // Import TableBuilder and Column

use crate::sheets::{events::UpdateCellEvent, resources::SheetRegistry};
use super::state::{EditorWindowState, ReviewChoice};
use super::ai_helpers::{advance_review_queue, exit_review_mode};

pub(super) fn draw_inline_ai_review_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    registry: &SheetRegistry,
    cell_update_writer: &mut EventWriter<UpdateCellEvent>,
) {
    // Determine active sheet (virtual or real)
    let active_sheet_name = if let Some(vctx) = state.virtual_structure_stack.last() { vctx.virtual_sheet_name.as_str() } else if let Some(s) = selected_sheet_name_clone { s.as_str() } else { exit_review_mode(state); return; };
    // Row index being reviewed
    let original_row_index = match state.current_ai_suggestion_edit_buffer { Some((idx,_)) => idx, None => { advance_review_queue(state); return; } };
    // Snapshot row & metadata
    let (original_data_cloned, metadata_cloned) = { let sheet_opt = registry.get_sheet(selected_category_clone, active_sheet_name); (sheet_opt.and_then(|s| s.grid.get(original_row_index)).cloned(), sheet_opt.and_then(|s| s.metadata.clone())) }; let metadata_opt = metadata_cloned.as_ref();
    // Prepare derived data outside UI closures to avoid multiple mutable borrows.
    let context_prefix_opt = state.ai_context_prefix_by_row.get(&original_row_index).cloned();
    let suggestion_indices_and_state = if let (Some(original_row), Some(metadata), Some((_idx, _suggestion_buf))) = (original_data_cloned.as_ref(), metadata_opt, state.current_ai_suggestion_edit_buffer.as_ref()) {
        // Determine visible columns (non-Structure) and also capture context-prefix (key) headers if present
        let visible: Vec<usize> = metadata.columns.iter().enumerate()
            .filter_map(|(i,c)| if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { None } else { Some(i) })
            .collect();
        let context_prefix = context_prefix_opt.as_ref().cloned().unwrap_or_default();
        Some((visible, original_row.clone(), metadata.clone()))
    } else { None };

    // --- UI Rendering ---
    let mut deferred_action: Option<&'static str> = None; // "apply" | "skip" | "cancel"
    egui::ScrollArea::horizontal().id_salt("ai_review_panel_scroll_area").auto_shrink([false, true]).show(ui, |ui| {
        egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
            ui.label(RichText::new(format!("Reviewing AI Suggestion for Original Row Index: {}", original_row_index)).heading());
            if let Some(prefix) = context_prefix_opt.as_ref() { if !prefix.is_empty() { ui.separator(); ui.colored_label(Color32::LIGHT_BLUE, "Context Columns (keys - not editable / not changed):"); egui::Grid::new("ai_context_prefix_grid").num_columns(2).striped(true).show(ui, |g| { for (hdr,val) in prefix.iter() { g.label(RichText::new(hdr).strong()); g.label(val); g.end_row(); } }); ui.separator(); } }
            ui.separator();
            if state.current_ai_suggestion_edit_buffer.is_none() || state.current_ai_suggestion_edit_buffer.as_ref().map_or(true, |(idx, _)| *idx != original_row_index) { ui.colored_label(Color32::YELLOW, "Review item changed, refreshing..."); return; }
            let (_, current_suggestion_mut) = state.current_ai_suggestion_edit_buffer.as_mut().unwrap();
            match suggestion_indices_and_state.as_ref() {
                Some((visible_col_indices, original_row, metadata)) => {
                    let num_cols = visible_col_indices.len();
                    if current_suggestion_mut.len() == metadata.columns.len() { let mut filtered=Vec::with_capacity(visible_col_indices.len()); for actual in visible_col_indices { filtered.push(current_suggestion_mut.get(*actual).cloned().unwrap_or_default()); } *current_suggestion_mut = filtered; }
                    else if current_suggestion_mut.len() < num_cols { current_suggestion_mut.resize(num_cols, String::new()); }
                    if state.ai_review_column_choices.len() != num_cols { state.ai_review_column_choices = vec![ReviewChoice::AI; num_cols]; }
                    let text_style = TextStyle::Body; let row_height = ui.text_style_height(&text_style);
                    // Build a resizable table with explicit columns to ensure alignment and resizing work properly
                    let mut table_builder = TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .cell_layout(egui::Layout::left_to_right(Align::Center))
                        .min_scrolled_height(0.0);
                    // First add prefix/context columns (if any) so they are resizable like normal columns
                    for _ in 0..context_prefix.len() { table_builder = table_builder.column(Column::initial(110.0).at_least(40.0).resizable(true).clip(true)); }
                    for _ in 0..num_cols { table_builder = table_builder.column(Column::initial(120.0).at_least(60.0).resizable(true).clip(true)); }
                    table_builder.header(20.0, |mut header| {
                        // Render context-prefix headers first
                        for (hdr, _val) in &context_prefix { header.col(|ui| { ui.strong(hdr); }); }
                        for (_display_idx, actual_idx) in visible_col_indices.iter().enumerate() { header.col(|ui| { let col_header = metadata.columns.get(*actual_idx).map_or_else(|| format!("Col {}", actual_idx+1), |c| c.header.clone()); ui.strong(col_header); }); }
                    })
                        .body(|mut body| {
                        body.row(row_height, |mut row| {
                            // context values (read-only)
                            for (_hdr, val) in &context_prefix { row.col(|ui| { ui.label(val); }); }
                            for (display_idx, actual_idx) in visible_col_indices.iter().enumerate() { row.col(|ui| { let original_value = original_row.get(*actual_idx).cloned().unwrap_or_default(); let current_choice = state.ai_review_column_choices[display_idx]; let is_different = original_value != current_suggestion_mut.get(display_idx).cloned().unwrap_or_default(); let display_text = if is_different && current_choice == ReviewChoice::AI { RichText::new(&original_value).strikethrough() } else { RichText::new(&original_value) }; ui.label(display_text).on_hover_text("Original Value"); }); }
                        });
                        body.row(row_height, |mut row| {
                            // empty cells under context prefix for AI edit row
                            for _ in 0..context_prefix.len() { row.col(|ui| { ui.label(" "); }); }
                            for (display_idx, actual_idx) in visible_col_indices.iter().enumerate() { row.col(|ui| { let original_value = original_row.get(*actual_idx).cloned().unwrap_or_default(); let ai_value_mut = current_suggestion_mut.get_mut(display_idx).expect("Suggestion vec exists"); let is_different = original_value != *ai_value_mut; ui.add(egui::TextEdit::singleline(ai_value_mut).desired_width(f32::INFINITY).text_color_opt(if is_different { Some(Color32::LIGHT_YELLOW) } else { None })); }); }
                        });
                        body.row(row_height, |mut row| {
                            // context prefix has no choice controls
                            for _ in 0..context_prefix.len() { row.col(|ui| { ui.label(" "); }); }
                            for display_idx in 0..num_cols { row.col(|ui| { ui.horizontal_centered(|ui| { let mut choice = state.ai_review_column_choices[display_idx]; if ui.radio_value(&mut choice, ReviewChoice::Original, "Original").clicked() { state.ai_review_column_choices[display_idx] = ReviewChoice::Original; } if ui.radio_value(&mut choice, ReviewChoice::AI, "AI").clicked() { state.ai_review_column_choices[display_idx] = ReviewChoice::AI; } }); }); }
                        });
                        });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("✅ Apply Chosen Changes").clicked() { deferred_action = Some("apply"); }
                        if ui.button("⏩ Skip This Row").clicked() { deferred_action = Some("skip"); }
                        if ui.button("🔁 Clear Key Context").on_hover_text("Remove stored key context (does not modify data; clears display only)").clicked() { state.ai_context_prefix_by_row.remove(&original_row_index); }
                        if !state.ai_review_queue.is_empty() {
                            ui.separator();
                            if ui.button("✅ Accept All Remaining").on_hover_text("Apply AI/original choices for current and all queued rows").clicked() { deferred_action = Some("apply_all_remaining"); }
                            if ui.button("🛑 Decline All Remaining").on_hover_text("Skip every remaining queued suggestion").clicked() { deferred_action = Some("skip_all_remaining"); }
                        }
                    });
                }
                None => { ui.colored_label(Color32::RED, "Original row or metadata missing."); }
            }
        });
    });

    // Perform deferred action after UI borrow ends
    if let Some(action) = deferred_action { match action { "apply" => {
        if let Some((visible_col_indices, original_row, _meta)) = suggestion_indices_and_state {
            if let Some((_idx, suggestion_buf)) = &state.current_ai_suggestion_edit_buffer {
                for (display_idx, actual_idx) in visible_col_indices.iter().enumerate() {
                    let choice = state.ai_review_column_choices.get(display_idx).cloned().unwrap_or(ReviewChoice::Original);
                    let original_cell_value = original_row.get(*actual_idx).cloned().unwrap_or_default();
                    let ai_cell_value = suggestion_buf.get(display_idx).cloned().unwrap_or_default();
                    let value_to_apply = match choice { ReviewChoice::Original => &original_cell_value, ReviewChoice::AI => &ai_cell_value };
                    let current_grid_value = registry.get_sheet(selected_category_clone, active_sheet_name).and_then(|s| s.grid.get(original_row_index)).and_then(|r| r.get(*actual_idx)).cloned().unwrap_or_default();
                        if *value_to_apply != current_grid_value {
                            info!(
                                "ApplyAll(single): row={} col={} val={}",
                                original_row_index,
                                *actual_idx,
                                value_to_apply
                            );
                            cell_update_writer.write(UpdateCellEvent { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), row_index: original_row_index, col_index: *actual_idx, new_value: value_to_apply.clone() });
                        }
                }
            }
        }
        advance_review_queue(state);
    }, "apply_all_remaining" => {
        // Helper to apply current row with existing choice selections (same as pressing Apply)
        let mut apply_current = |state: &mut EditorWindowState| {
            if let Some((visible_col_indices, original_row, _meta)) = suggestion_indices_and_state.clone() {
                if let Some((idx, suggestion_buf)) = &state.current_ai_suggestion_edit_buffer {
                    for (display_idx, actual_idx) in visible_col_indices.iter().enumerate() {
                        let choice = state.ai_review_column_choices.get(display_idx).cloned().unwrap_or(ReviewChoice::AI);
                        let original_cell_value = original_row.get(*actual_idx).cloned().unwrap_or_default();
                        let ai_cell_value = suggestion_buf.get(display_idx).cloned().unwrap_or_default();
                        let value_to_apply = match choice { ReviewChoice::Original => &original_cell_value, ReviewChoice::AI => &ai_cell_value };
                        let current_grid_value = registry.get_sheet(selected_category_clone, active_sheet_name).and_then(|s| s.grid.get(*idx)).and_then(|r| r.get(*actual_idx)).cloned().unwrap_or_default();
                        if *value_to_apply != current_grid_value {
                            info!("ApplyAll(single): row={} col={} val={}", *idx, *actual_idx, value_to_apply);
                            cell_update_writer.write(UpdateCellEvent { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), row_index: *idx, col_index: *actual_idx, new_value: value_to_apply.clone() });
                        }
                    }
                }
            }
        };
        // 1) Apply current row respecting user selections
        apply_current(state);
        // 2) For each remaining queued row: setup next, normalize suggestion buffer, set choices to AI for all, apply
        loop {
            // Move to next; exit if finished
            advance_review_queue(state);
            if state.ai_current_review_index.is_none() { break; }
            // Build fresh context for newly current row
            let (visible_col_indices, original_row, num_cols) = if let Some((cur_idx, (cat, sheet))) = state.ai_current_review_index.map(|_| (state.ai_current_review_index.unwrap(), (selected_category_clone.clone(), active_sheet_name.to_string()))) {
                // Resolve original row index and metadata
                let orig_row_idx = state.current_ai_suggestion_edit_buffer.as_ref().map(|(ridx, _)| *ridx).unwrap_or(0);
                let (visible, original, mcols) = if let Some(sheet_ref) = registry.get_sheet(&cat, &sheet) {
                    let meta_opt = sheet_ref.metadata.as_ref();
                    let visible: Vec<usize> = if let Some(meta) = meta_opt {
                        meta.columns.iter().enumerate()
                            .filter_map(|(i,c)| if matches!(c.validator, Some(crate::sheets::definitions::ColumnValidator::Structure)) { None } else { Some(i) })
                            .collect()
                    } else { (0..sheet_ref.grid.first().map(|r| r.len()).unwrap_or(0)).collect() };
                    let orig = sheet_ref.grid.get(orig_row_idx).cloned().unwrap_or_default();
                    (visible, orig, meta_opt.map(|m| m.columns.len()).unwrap_or(0))
                } else { (Vec::new(), Vec::new(), 0) };
                (visible, original, mcols)
            } else { (Vec::new(), Vec::new(), 0) };
            // Normalize current suggestion buffer to visible columns length
            if let Some((_row_idx, suggestion_buf)) = &mut state.current_ai_suggestion_edit_buffer {
                if suggestion_buf.len() == num_cols && !visible_col_indices.is_empty() {
                    let mut filtered: Vec<String> = Vec::with_capacity(visible_col_indices.len());
                    for actual in &visible_col_indices { filtered.push(suggestion_buf.get(*actual).cloned().unwrap_or_default()); }
                    *suggestion_buf = filtered;
                } else if suggestion_buf.len() < visible_col_indices.len() {
                    suggestion_buf.resize(visible_col_indices.len(), String::new());
                }
            }
            // Set choices to AI for all columns
            state.ai_review_column_choices = vec![ReviewChoice::AI; visible_col_indices.len()];
            // Apply for this row
            if let Some((row_idx, suggestion_buf)) = &state.current_ai_suggestion_edit_buffer {
                for (display_idx, actual_idx) in visible_col_indices.iter().enumerate() {
                    let ai_cell_value = suggestion_buf.get(display_idx).cloned().unwrap_or_default();
                    let current_grid_value = registry.get_sheet(selected_category_clone, active_sheet_name).and_then(|s| s.grid.get(*row_idx)).and_then(|r| r.get(*actual_idx)).cloned().unwrap_or_default();
                    if ai_cell_value != current_grid_value {
                        info!("ApplyAll(single-next): row={} col={} val={}", *row_idx, *actual_idx, ai_cell_value);
                        cell_update_writer.write(UpdateCellEvent { category: selected_category_clone.clone(), sheet_name: active_sheet_name.to_string(), row_index: *row_idx, col_index: *actual_idx, new_value: ai_cell_value });
                    }
                }
            }
            // Loop continues to advance further
        }
        // After loop, we're out of review mode
    }, "skip" => advance_review_queue(state), "skip_all_remaining" => {
        // Simply exit review mode without applying current or remaining
        exit_review_mode(state);
    }, _ => {} } }
}
