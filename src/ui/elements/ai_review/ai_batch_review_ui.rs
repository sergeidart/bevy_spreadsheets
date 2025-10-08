use crate::sheets::definitions::{ColumnValidator, StructureFieldDefinition};
use crate::sheets::events::{AddSheetRowRequest, OpenStructureViewEvent, UpdateCellEvent};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::ai_review::handlers::{
    finalize_if_empty, process_existing_accept, process_existing_decline, process_new_accept,
    process_new_decline,
};
use crate::ui::elements::ai_review::header_actions::draw_header_actions;
use crate::ui::elements::ai_review::render::row_render::{build_blocks, render_rows, RowContext};
use crate::ui::elements::ai_review::structure_review_helpers::{
    build_structure_ancestor_keys, build_structure_columns, convert_structure_to_reviews,
};
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState, NewRowReview, ReviewChoice, RowReview};
use bevy::prelude::*;
use bevy_egui::egui::{self, RichText};

#[derive(Debug, Clone, Copy)]
pub enum ColumnEntry {
    Regular(usize),    // Index into non_structure_columns
    Structure(usize),  // Original column index from sheet metadata
}

fn cancel_batch(state: &mut EditorWindowState) {
    state.ai_batch_review_active = false;
    state.ai_mode = AiModeState::Idle;
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    state.ai_selected_rows.clear();
    state.ai_structure_detail_context = None;
    // Clear Phase 1/2 state
    state.ai_phase1_intermediate = None;
    state.ai_expecting_phase2_result = false;
    // Also reset broader interaction modes and selections so the UI returns to normal (hides "Exit AI").
    state.reset_interaction_modes_and_selections();
}

/// Persist the temporary structure row reviews (in structure detail mode) back into the corresponding StructureReviewEntry.
/// We only store merged decisions (Original vs AI per column) into the entry.merged_rows and flag acceptance/rejection on bulk actions.
pub fn persist_structure_detail_changes(
    state: &mut EditorWindowState,
    detail_ctx: &crate::ui::elements::editor::state::StructureDetailContext,
) {
    // Locate the matching structure review entry mutably
    if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| {
        match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
            (Some(a), Some(b)) if a == b => sr.structure_path == detail_ctx.structure_path,
            (None, None) => sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                && sr.structure_path == detail_ctx.structure_path,
            _ => false,
        }
    }) {
        // Reconstruct merged_rows from current temp ai_row_reviews / ai_new_row_reviews.
        // In structure detail mode only ai_row_reviews are populated (existing rows) and ai_new_row_reviews for appended ones.
        // Convert RowReview -> merged row using choices (Original keeps original cell, AI picks ai cell).
        let mut merged_rows: Vec<Vec<String>> = Vec::new();
        let mut differences: Vec<Vec<bool>> = Vec::new();
        for rr in &state.ai_row_reviews {
            // We need the original and ai row complete; they are stored entirely (not only diffs) in RowReview
            let mut merged = rr.original.clone();
            let mut diff_flags = vec![false; merged.len()];
            for (pos, &col_idx) in rr.non_structure_columns.iter().enumerate() {
                if let Some(choice) = rr.choices.get(pos) {
                    match choice {
                        ReviewChoice::AI => {
                            if let (Some(ai_val), Some(slot)) = (rr.ai.get(pos), merged.get_mut(col_idx)) {
                                *slot = ai_val.clone();
                            }
                            diff_flags[col_idx] = true;
                        }
                        ReviewChoice::Original => {
                            // keep original; mark diff flag if original != ai for trace
                            if let (Some(orig_val), Some(ai_val)) = (
                                rr.original.get(pos),
                                rr.ai.get(pos),
                            ) {
                                if orig_val != ai_val { diff_flags[col_idx] = true; }
                            }
                        }
                    }
                }
            }
    // Collect row-level actions injected via temporary vectors on context (planned extension). For now detect button presses inline.
    // Pseudocode placeholder hooking into existing row render calls if they push events.
            merged_rows.push(merged);
            differences.push(diff_flags);
        }
        // Append new rows (always AI rows)
        for (nr_idx, nr) in state.ai_new_row_reviews.iter().enumerate() {
            // Build merged row directly from AI values (no column index mapping needed)
            // The AI row already has the correct structure for this schema
            let merged = nr.ai.clone();
            info!("persist_structure_detail: adding new row {}: len={}, data={:?}", nr_idx, merged.len(), merged);
            let diff_flags = vec![true; merged.len()]; // All cells are from AI (new row)
            merged_rows.push(merged);
            differences.push(diff_flags);
        }
        if !merged_rows.is_empty() {
            info!("persist_structure_detail: updating entry with {} merged_rows, schema_headers.len={}", merged_rows.len(), entry.schema_headers.len());
            entry.merged_rows = merged_rows;
            entry.differences = differences; // reflect user's latest decision set
            entry.has_changes = entry.differences.iter().flatten().any(|b| *b);
        }
    }
}

/// Apply accept/decline for a single existing structure row inside detail mode.
fn structure_row_apply_existing(
    entry: &mut crate::ui::elements::editor::state::StructureReviewEntry,
    rr: &RowReview,
    accept: bool,
) {
    let row_idx = rr.row_index;
    if row_idx >= entry.merged_rows.len() || row_idx >= entry.original_rows.len() { return; }
    if accept {
        // Merge based on choices
        for (pos, &col_idx) in rr.non_structure_columns.iter().enumerate() {
            if let Some(choice) = rr.choices.get(pos) {
                if matches!(choice, ReviewChoice::AI) {
                    if let (Some(ai_val), Some(slot)) = (rr.ai.get(pos), entry.merged_rows[row_idx].get_mut(col_idx)) {
                        *slot = ai_val.clone();
                    }
                } else {
                    // Original - ensure merged matches original
                    if let (Some(orig_val), Some(slot)) = (rr.original.get(pos), entry.merged_rows[row_idx].get_mut(col_idx)) {
                        *slot = orig_val.clone();
                    }
                }
            }
        }
    } else {
        // Decline -> reset merged to original row
        if row_idx < entry.original_rows.len() {
            entry.merged_rows[row_idx] = entry.original_rows[row_idx].clone();
        }
    }
    // Clear differences for this row (resolved)
    if row_idx < entry.differences.len() {
        for flag in &mut entry.differences[row_idx] { *flag = false; }
    }
}

/// Apply accept/decline for a single new structure row (AI added) inside detail mode.
fn structure_row_apply_new(
    entry: &mut crate::ui::elements::editor::state::StructureReviewEntry,
    new_row_local_idx: usize, // index within temp ai_new_row_reviews list
    ai_new_row_reviews: &[NewRowReview],
    accept: bool,
) {
    // New rows in entry start after original_rows.len()
    let base = entry.original_rows.len();
    let target_idx = base + new_row_local_idx;
    if target_idx >= entry.merged_rows.len() { return; }
    if accept {
        if let Some(nr) = ai_new_row_reviews.get(new_row_local_idx) {
            // Build merged row from AI values
            let mut merged = entry.merged_rows[target_idx].clone();
            if merged.len() < nr.ai.len() { merged.resize(nr.ai.len(), String::new()); }
            for (pos, &col_idx) in nr.non_structure_columns.iter().enumerate() {
                if let (Some(ai_val), Some(slot)) = (nr.ai.get(pos), merged.get_mut(col_idx)) { *slot = ai_val.clone(); }
            }
            entry.merged_rows[target_idx] = merged;
        }
    } else {
        // Decline new row: remove it entirely from structure suggestion arrays
        entry.ai_rows.remove(target_idx);
        entry.merged_rows.remove(target_idx);
        entry.differences.remove(target_idx);
        // Also need to adjust any placeholders: original_rows has no entry for new rows so nothing to remove there.
        return; // early return so we don't try to clear diff flags (already removed)
    }
    if target_idx < entry.differences.len() { for flag in &mut entry.differences[target_idx] { *flag = false; } }
}


pub(crate) fn draw_ai_batch_review_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    selected_sheet_name_clone: &Option<String>,
    registry: &SheetRegistry,
    _open_structure_writer: &mut EventWriter<OpenStructureViewEvent>,
    cell_update_writer: &mut EventWriter<UpdateCellEvent>,
    add_row_writer: &mut EventWriter<AddSheetRowRequest>,
) {
    if !state.ai_batch_review_active {
        return;
    }

    // Detect which of the three AI review modes we're in:
    // 1. Structure Detail Mode: drilling down into a structure from AI review (has ai_structure_detail_context)
    // 2. Virtual Structure Review: reviewing rows from within a virtual structure sheet (has virtual_structure_stack)
    // 3. Regular Review: reviewing rows from root sheet (neither of the above)
    let in_structure_detail_mode = state.ai_structure_detail_context.is_some();
    let in_virtual_structure_review = !in_structure_detail_mode && !state.virtual_structure_stack.is_empty();
    let in_structure_mode = in_structure_detail_mode; // Keep old variable name for compatibility
    if in_structure_mode {
        // Hydrate once when entering
        if let Some(detail_ctx) = &mut state.ai_structure_detail_context {
            if !detail_ctx.hydrated {
                let structure_entry = state.ai_structure_reviews.iter().find(|sr| {
                    match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
                        (Some(a), Some(b)) if a == b => sr.structure_path == detail_ctx.structure_path,
                        (None, None) => sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                            && sr.structure_path == detail_ctx.structure_path,
                        _ => false,
                    }
                }).cloned();
                if let Some(entry) = structure_entry {
                    // Restore the saved top-level reviews first (in case we went back and forth)
                    state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
                    state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
                    // Now replace with structure-specific reviews
                    let (temp_row_reviews, temp_new_row_reviews) = convert_structure_to_reviews(&entry);
                    state.ai_row_reviews = temp_row_reviews;
                    state.ai_new_row_reviews = temp_new_row_reviews;
                    detail_ctx.hydrated = true;
                } else {
                    state.ai_structure_detail_context = None; // entry missing
                }
            }
        }
    }

    // Auto-exit if nothing left (check for undecided structures too)
    let has_undecided_structures = state.ai_structure_reviews.iter().any(|entry| entry.is_undecided());
    if state.ai_row_reviews.is_empty() && state.ai_new_row_reviews.is_empty() && !in_structure_mode && !has_undecided_structures {
        cancel_batch(state); return;
    }

    // Resolve active sheet (respect virtual structure stack). Use owned String to avoid long-lived borrows of `state`.
    let active_sheet_name: String = if let Some(vctx) = state.virtual_structure_stack.last() {
        vctx.virtual_sheet_name.clone()
    } else if let Some(s) = selected_sheet_name_clone {
        s.clone()
    } else {
        if in_structure_mode {
            if let Some(ref detail_ctx) = state.ai_structure_detail_context {
                state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
                state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
            }
        }
        return;
    };

    // Determine a canonical non-structure column ordering from first review (they should all match)
    let mut union_cols: Vec<usize> = state
        .ai_row_reviews
        .first()
        .map(|r| r.non_structure_columns.clone())
        .unwrap_or_else(|| {
            state
                .ai_new_row_reviews
                .first()
                .map(|r| r.non_structure_columns.clone())
                .unwrap_or_default()
        });
    union_cols.sort_unstable();
    union_cols.dedup();

    // Build merged column list - different logic for structure mode vs normal mode
    let (merged_columns, structure_schema) = if in_structure_mode {
        // In structure detail mode: build columns from structure schema
        build_structure_columns(&union_cols, &state.ai_structure_detail_context, false, "", selected_category_clone, registry)
    } else if in_virtual_structure_review {
        // In virtual structure review: filter out structure columns
        build_structure_columns(&union_cols, &None, true, &active_sheet_name, selected_category_clone, registry)
    } else {
        // Normal mode: build columns from sheet metadata
        // IMPORTANT: Only show columns that are BOTH included in metadata AND present in union_cols (actually sent)
        let cols = if let Some(sheet) = registry.get_sheet(selected_category_clone, &active_sheet_name) {
            if let Some(metadata) = &sheet.metadata {
                let mut result = Vec::new();
                for (col_idx, col_def) in metadata.columns.iter().enumerate() {
                    let is_structure = matches!(col_def.validator, Some(ColumnValidator::Structure));
                    
                    // Only show columns that were actually sent
                    if is_structure {
                        // Structure columns: Check if this structure path was planned for sending
                        // Structure data is sent separately via ai_planned_structure_paths
                        let structure_path = vec![col_idx];
                        if state.ai_planned_structure_paths.contains(&structure_path) {
                            result.push(ColumnEntry::Structure(col_idx));
                        }
                    } else if union_cols.contains(&col_idx) {
                        // Regular columns: only show if present in union_cols (actually sent to AI)
                        result.push(ColumnEntry::Regular(col_idx));
                    }
                }
                result
            } else {
                union_cols.iter().map(|&idx| ColumnEntry::Regular(idx)).collect()
            }
        } else {
            union_cols.iter().map(|&idx| ColumnEntry::Regular(idx)).collect()
        };
        (cols, Vec::new())
    };

    // Gather ancestor key columns
    let mut ancestor_key_columns: Vec<(String, String)> = Vec::new();
    
    if in_structure_mode {
        if let Some(ref detail_ctx) = state.ai_structure_detail_context {
            ancestor_key_columns = build_structure_ancestor_keys(
                detail_ctx,
                &state,
                selected_category_clone,
                registry,
                &detail_ctx.saved_row_reviews,
                &detail_ctx.saved_new_row_reviews,
            );
        }
    } else if let Some(last_ctx) = state.virtual_structure_stack.last() {
        // Normal virtual structure stack logic
        if last_ctx.virtual_sheet_name == active_sheet_name {
            for vctx in &state.virtual_structure_stack {
                if let Some(parent_sheet) =
                    registry.get_sheet(selected_category_clone, &vctx.parent.parent_sheet)
                {
                    if let (Some(parent_meta), Some(parent_row)) = (
                        &parent_sheet.metadata,
                        parent_sheet.grid.get(vctx.parent.parent_row),
                    ) {
                        if let Some(struct_col_def) =
                            parent_meta.columns.get(vctx.parent.parent_col)
                        {
                            if let Some(key_col_idx) =
                                struct_col_def.structure_key_parent_column_index
                            {
                                if let Some(key_col_def) = parent_meta.columns.get(key_col_idx) {
                                    let value =
                                        parent_row.get(key_col_idx).cloned().unwrap_or_default();
                                    ancestor_key_columns.push((key_col_def.header.clone(), value));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Recompute pending merge flag & structure state
    state.ai_batch_has_undecided_merge = state
        .ai_new_row_reviews
        .iter()
        .any(|nr| nr.duplicate_match_row.is_some() && !nr.merge_decided);
    // Check for undecided structures - structures that have changes but haven't been decided yet
    let undecided_structures = state
        .ai_structure_reviews
        .iter()
        .any(|entry| entry.is_undecided());

    let show_pending_structures = if in_structure_mode { false } else { undecided_structures };
    let actions = draw_header_actions(ui, state, show_pending_structures);

    if actions.accept_all {
        if in_structure_mode {
            if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
                // Persist respects user's cell-by-cell choices (Original vs AI)
                persist_structure_detail_changes(state, detail_ctx);
                if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
                    (Some(a), Some(b)) if a == b => sr.structure_path == detail_ctx.structure_path,
                    (None, None) => sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX) && sr.structure_path == detail_ctx.structure_path,
                    _ => false,
                }) {
                    // Mark as accepted and decided, but don't override has_changes - it was calculated by persist_structure_detail_changes
                    entry.accepted = true; entry.rejected = false; entry.decided = true;
                }
                // Restore top-level reviews
                state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
                state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
            }
            state.ai_structure_detail_context = None; // back out
            // Don't auto-accept parent rows - let user accept them separately
        } else {
            let existing_indices: Vec<usize> = (0..state.ai_row_reviews.len()).collect();
            let new_indices: Vec<usize> = state.ai_new_row_reviews.iter().enumerate().filter(|(_, nr)| nr.duplicate_match_row.is_none() || nr.merge_decided).map(|(i, _)| i).collect();
            process_existing_accept(&existing_indices, state, selected_category_clone, &active_sheet_name, cell_update_writer);
            process_new_accept(&new_indices, state, selected_category_clone, &active_sheet_name, cell_update_writer, add_row_writer);
            cancel_batch(state); return;
        }
    }

    let (blocks, group_starts) = build_blocks(state);

    // Get the active sheet grid for structure column access
    let active_sheet_grid = registry
        .get_sheet(selected_category_clone, &active_sheet_name)
        .map(|sheet| &sheet.grid);

    // Wrap table in ScrollArea to ensure proper scrolling in all modes
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .id_salt("ai_batch_review_table_mod")
        .show(ui, |ui| {
            use bevy_egui::egui::Align;
            use egui_extras::{Column, TableBuilder};

            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(Align::Center))
                .min_scrolled_height(0.0);

            builder = builder.column(Column::exact(120.0));
            for _ in &ancestor_key_columns {
                builder = builder.column(Column::initial(120.0).at_least(80.0).resizable(true));
            }
            for _ in &merged_columns {
                builder = builder.column(Column::initial(120.0).at_least(80.0).resizable(true));
            }

            // Add header row
            let row_height = 25.0;
            builder.header(row_height, |mut header_row| {
                // First column: Action/Status header
                header_row.col(|ui| {
                    ui.label(RichText::new("Action").strong());
                    let rect = ui.max_rect();
                    let y = rect.bottom();
                    ui.painter().hline(rect.x_range(), y, ui.visuals().widgets.noninteractive.bg_stroke);
                });
                
                // Ancestor key columns (green)
                for (key_header, value) in &ancestor_key_columns {
                    header_row.col(|ui| {
                        let r = ui.colored_label(egui::Color32::from_rgb(0, 170, 0), RichText::new(key_header).strong());
                        if !value.is_empty() {
                            r.on_hover_text(format!("Key value: {}", value));
                        } else {
                            r.on_hover_text(format!("Key column: {}", key_header));
                        }
                        let rect = ui.max_rect();
                        let y = rect.bottom();
                        ui.painter().hline(rect.x_range(), y, ui.visuals().widgets.noninteractive.bg_stroke);
                    });
                }
                
                // Regular and structure columns
                let sheet_metadata = registry
                    .get_sheet(selected_category_clone, &active_sheet_name)
                    .and_then(|sheet| sheet.metadata.as_ref());
                
                for col_entry in &merged_columns {
                    header_row.col(|ui| {
                        let header_text = match col_entry {
                            ColumnEntry::Regular(col_idx) => {
                                if in_structure_mode {
                                    // In structure mode, use structure schema
                                    structure_schema.get(*col_idx)
                                        .map(|field| field.header.as_str())
                                        .unwrap_or("?")
                                } else {
                                    // In normal mode, use sheet metadata
                                    sheet_metadata
                                        .and_then(|meta| meta.columns.get(*col_idx))
                                        .map(|col| col.header.as_str())
                                        .unwrap_or("?")
                                }
                            },
                            ColumnEntry::Structure(col_idx) => {
                                if in_structure_mode {
                                    // In structure mode, use structure schema
                                    structure_schema.get(*col_idx)
                                        .map(|field| field.header.as_str())
                                        .unwrap_or("Structure")
                                } else {
                                    // In normal mode, use sheet metadata
                                    sheet_metadata
                                        .and_then(|meta| meta.columns.get(*col_idx))
                                        .map(|col| col.header.as_str())
                                        .unwrap_or("Structure")
                                }
                            },
                        };
                        
                        ui.label(RichText::new(header_text).strong());
                        let rect = ui.max_rect();
                        let y = rect.bottom();
                        ui.painter().hline(rect.x_range(), y, ui.visuals().widgets.noninteractive.bg_stroke);
                    });
                }
            })
            .body(|mut body| {
                let mut existing_accept = Vec::new();
                let mut existing_cancel = Vec::new();
                let mut new_accept = Vec::new();
                let mut new_cancel = Vec::new();
                let mut structure_nav_clicked: Option<(Option<usize>, Option<usize>, Vec<usize>)> = None;
                let mut structure_quick_accept: Vec<(Option<usize>, Option<usize>, Vec<usize>)> = Vec::new();

                // Clone structure reviews for reading (they're only needed for display, not mutation)
                let ai_structure_reviews = state.ai_structure_reviews.clone();
                
                // Get sheet metadata for column validators
                let sheet_metadata = registry
                    .get_sheet(selected_category_clone, &active_sheet_name)
                    .and_then(|sheet| sheet.metadata.as_ref());
                
                // Pre-fetch all linked column options
                use crate::ui::widgets::linked_column_cache::{self, CacheResult};
                let mut linked_column_options = std::collections::HashMap::new();
                
                // For nested structures, use structure_schema; otherwise use sheet metadata columns
                if in_structure_mode && !structure_schema.is_empty() {
                    // In structure detail mode: get validators from structure schema
                    for col_entry in &merged_columns {
                        if let ColumnEntry::Regular(actual_col) = col_entry {
                            if let Some(field_def) = structure_schema.get(*actual_col) {
                                if let Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) = &field_def.validator {
                                    if let CacheResult::Success { raw, .. } = linked_column_cache::get_or_populate_linked_options(
                                        target_sheet_name,
                                        *target_column_index,
                                        registry,
                                        state,
                                    ) {
                                        linked_column_options.insert(*actual_col, raw);
                                    }
                                }
                            }
                        }
                    }
                } else if let Some(meta) = sheet_metadata {
                    // Normal mode or virtual structure review: get validators from sheet metadata
                    for col_entry in &merged_columns {
                        if let ColumnEntry::Regular(actual_col) = col_entry {
                            if let Some(col_def) = meta.columns.get(*actual_col) {
                                if let Some(ColumnValidator::Linked { target_sheet_name, target_column_index }) = &col_def.validator {
                                    if let CacheResult::Success { raw, .. } = linked_column_cache::get_or_populate_linked_options(
                                        target_sheet_name,
                                        *target_column_index,
                                        registry,
                                        state,
                                    ) {
                                        linked_column_options.insert(*actual_col, raw);
                                    }
                                }
                            }
                        }
                    }
                }
                
                render_rows(
                    &mut body,
                    RowContext {
                        state,
                        ancestor_key_columns: &ancestor_key_columns,
                        merged_columns: &merged_columns,
                        blocks: &blocks,
                        group_start_indices: &group_starts,
                        existing_accept: &mut existing_accept,
                        existing_cancel: &mut existing_cancel,
                        new_accept: &mut new_accept,
                        new_cancel: &mut new_cancel,
                        active_sheet_grid,
                        ai_structure_reviews: &ai_structure_reviews,
                        sheet_metadata,
                        registry,
                        linked_column_options: &linked_column_options,
                        structure_nav_clicked: &mut structure_nav_clicked,
                        structure_quick_accept: &mut structure_quick_accept,
                    },
                );

                if actions.decline_all {
                    if in_structure_mode {
                        if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
                            if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
                                (Some(a), Some(b)) if a == b => sr.structure_path == detail_ctx.structure_path,
                                (None, None) => sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX) && sr.structure_path == detail_ctx.structure_path,
                                _ => false,
                            }) { entry.accepted = false; entry.rejected = true; entry.decided = true; }
                            // Restore top-level reviews
                            state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
                            state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
                        }
                        state.ai_structure_detail_context = None;
                    } else {
                        existing_cancel.extend(0..state.ai_row_reviews.len());
                        new_cancel.extend(0..state.ai_new_row_reviews.len());
                    }
                }

                // Process quick accepts from context menu
                for (parent_row, parent_new_row, path) in structure_quick_accept {
                    if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| {
                        sr.parent_row_index == parent_row.unwrap_or(usize::MAX)
                        && sr.parent_new_row_index == parent_new_row
                        && sr.structure_path == path
                    }) {
                        // Mark as accepted and decided
                        entry.accepted = true;
                        entry.rejected = false;
                        entry.decided = true;
                        // Use merged_rows if populated (contains user edits), otherwise use ai_rows
                        if entry.merged_rows.is_empty() {
                            entry.merged_rows = entry.ai_rows.clone();
                        }
                    }
                }

                existing_accept.sort_unstable();
                existing_accept.dedup();
                existing_cancel.sort_unstable();
                existing_cancel.dedup();
                existing_cancel.retain(|i| !existing_accept.contains(i));

                new_accept.sort_unstable();
                new_accept.dedup();
                new_cancel.sort_unstable();
                new_cancel.dedup();
                new_cancel.retain(|i| !new_accept.contains(i));

                if in_structure_mode {
                    if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
                        if let Some(entry_index) = state.ai_structure_reviews.iter().position(|sr| match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
                            (Some(a), Some(b)) if a == b => sr.structure_path == detail_ctx.structure_path,
                            (None, None) => sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX) && sr.structure_path == detail_ctx.structure_path,
                            _ => false,
                        }) {
                            let entry_ptr: *mut _ = &mut state.ai_structure_reviews[entry_index];
                            // Safe because we don't move state.ai_structure_reviews while using entry_ptr
                            unsafe {
                                let entry = &mut *entry_ptr;
                                // Existing accepts
                                for &idx in &existing_accept { if let Some(rr) = state.ai_row_reviews.get(idx) { structure_row_apply_existing(entry, rr, true); } }
                                for &idx in &existing_cancel { if let Some(rr) = state.ai_row_reviews.get(idx) { structure_row_apply_existing(entry, rr, false); } }
                                // Remove existing rows from temp view (reverse order to keep indices valid)
                                if !existing_accept.is_empty() || !existing_cancel.is_empty() {
                                    let mut to_remove: Vec<usize> = Vec::new();
                                    to_remove.extend(existing_accept.iter().cloned());
                                    to_remove.extend(existing_cancel.iter().cloned());
                                    to_remove.sort_unstable();
                                    to_remove.dedup();
                                    for idx in to_remove.into_iter().rev() { 
                                        if idx < state.ai_row_reviews.len() { 
                                            state.ai_row_reviews.remove(idx); 
                                        }
                                    }
                                    // CRITICAL: Update row_index in remaining RowReview entries after removal
                                    // Row indices must match their position in the arrays (original_rows, merged_rows, etc.)
                                    for (new_idx, rr) in state.ai_row_reviews.iter_mut().enumerate() {
                                        rr.row_index = new_idx;
                                    }
                                }
                                // New rows
                                for &idx in &new_accept { structure_row_apply_new(entry, idx, &state.ai_new_row_reviews, true); }
                                for &idx in &new_cancel { structure_row_apply_new(entry, idx, &state.ai_new_row_reviews, false); }
                                // Remove accepted/declined new rows from temp view to mimic top-level behavior
                                if !new_accept.is_empty() || !new_cancel.is_empty() {
                                    let mut to_remove: Vec<usize> = Vec::new();
                                    to_remove.extend(new_accept.iter().cloned());
                                    to_remove.extend(new_cancel.iter().cloned());
                                    to_remove.sort_unstable();
                                    to_remove.dedup();
                                    for idx in to_remove.into_iter().rev() { if idx < state.ai_new_row_reviews.len() { state.ai_new_row_reviews.remove(idx); } }
                                }
                                // Mark entry has changes
                                entry.has_changes = true;
                                // Auto-mark decided and accepted if no remaining temp rows left
                                let no_temp_rows = state.ai_row_reviews.is_empty() && state.ai_new_row_reviews.is_empty();
                                if no_temp_rows {
                                    entry.decided = true;
                                    if entry.differences.iter().all(|row| row.iter().all(|f| !*f)) {
                                        entry.accepted = true; entry.rejected = false;
                                    }
                                    
                                    // Exit structure detail mode and restore parent level
                                    if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
                                        state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
                                        state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
                                        state.ai_structure_detail_context = None;
                                    }
                                }
                            }
                        }
                    }
                } else {
                    if !existing_accept.is_empty() {
                        process_existing_accept(
                            &existing_accept,
                            state,
                            selected_category_clone,
                            &active_sheet_name,
                            cell_update_writer,
                        );
                    }

                    if !existing_cancel.is_empty() {
                        process_existing_decline(&existing_cancel, state);
                    }

                    if !new_accept.is_empty() {
                        process_new_accept(
                            &new_accept,
                            state,
                            selected_category_clone,
                            &active_sheet_name,
                            cell_update_writer,
                            add_row_writer,
                        );
                    }

                    if !new_cancel.is_empty() {
                        process_new_decline(&new_cancel, state);
                    }
                }

                // Handle structure navigation click
                if let Some((parent_row_idx, parent_new_row_idx, structure_path)) = structure_nav_clicked {
                    // Find the structure entry to get root sheet information
                    let structure_entry = state.ai_structure_reviews.iter().find(|sr| {
                        match (parent_row_idx, parent_new_row_idx) {
                            (Some(pr), None) => sr.parent_row_index == pr && sr.parent_new_row_index.is_none() && sr.structure_path == structure_path,
                            (None, Some(pnr)) => sr.parent_new_row_index == Some(pnr) && sr.structure_path == structure_path,
                            _ => false,
                        }
                    });
                    
                    if let Some(entry) = structure_entry {
                        // Save current top-level reviews before entering structure mode
                        let saved_row_reviews = state.ai_row_reviews.clone();
                        let saved_new_row_reviews = state.ai_new_row_reviews.clone();
                        state.ai_structure_detail_context = Some(crate::ui::elements::editor::state::StructureDetailContext {
                            root_category: entry.root_category.clone(),
                            root_sheet: entry.root_sheet.clone(),
                            parent_row_index: parent_row_idx,
                            parent_new_row_index: parent_new_row_idx,
                            structure_path,
                            hydrated: false,
                            saved_row_reviews,
                            saved_new_row_reviews,
                        });
                    }
                }
            });
        });

    // Restore original reviews if we were in structure mode
    // Do NOT restore saved reviews; structure mode maintains its own working copy until exit

    finalize_if_empty(state);
    if !state.ai_batch_review_active {
        cancel_batch(state);
    }
}
