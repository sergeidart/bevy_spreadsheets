use crate::sheets::definitions::ColumnValidator;
use crate::sheets::events::{AddSheetRowRequest, UpdateCellEvent};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::cache_handlers::cancel_batch;
use crate::sheets::systems::ai_review::structure_persistence::persist_structure_detail_changes;
use crate::ui::elements::ai_review::handlers::{
    process_existing_accept, process_new_accept,
};
use crate::ui::elements::ai_review::structure_review_helpers::{
    build_structure_ancestor_keys, build_structure_columns, convert_structure_to_reviews,
};
use crate::ui::elements::editor::state::{EditorWindowState, StructureDetailContext};
use bevy::prelude::*;

#[derive(Debug, Clone, Copy)]
pub enum ColumnEntry {
    Regular(usize),   // Index into non_structure_columns
    Structure(usize), // Original column index from sheet metadata
}

/// Hydrates the structure detail context by loading structure-specific reviews
pub fn hydrate_structure_detail_if_needed(state: &mut EditorWindowState) {
    if let Some(detail_ctx) = &mut state.ai_structure_detail_context {
        if !detail_ctx.hydrated {
            let structure_entry = state
                .ai_structure_reviews
                .iter()
                .find(
                    |sr| match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
                        (Some(a), Some(b)) if a == b => {
                            sr.structure_path == detail_ctx.structure_path
                        }
                        (None, None) => {
                            sr.parent_row_index
                                == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                                && sr.structure_path == detail_ctx.structure_path
                        }
                        _ => false,
                    },
                )
                .cloned();
            if let Some(entry) = structure_entry {
                // Restore the saved top-level reviews first (in case we went back and forth)
                state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
                state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
                // Now replace with structure-specific reviews
                let (temp_row_reviews, temp_new_row_reviews) =
                    convert_structure_to_reviews(&entry);
                state.ai_row_reviews = temp_row_reviews;
                state.ai_new_row_reviews = temp_new_row_reviews;
                detail_ctx.hydrated = true;
            } else {
                state.ai_structure_detail_context = None; // entry missing
            }
        }
    }
}

/// Checks if the review should auto-exit due to no remaining items
pub fn should_auto_exit(state: &EditorWindowState, in_structure_mode: bool) -> bool {
    let has_undecided_structures = state
        .ai_structure_reviews
        .iter()
        .any(|entry| entry.is_undecided());
    
    state.ai_row_reviews.is_empty()
        && state.ai_new_row_reviews.is_empty()
        && !in_structure_mode
        && !has_undecided_structures
}

/// Resolves the active sheet name from state, considering virtual structure stack
pub fn resolve_active_sheet_name(
    state: &mut EditorWindowState,
    selected_sheet_name_clone: &Option<String>,
    in_structure_mode: bool,
) -> Option<String> {
    if let Some(vctx) = state.virtual_structure_stack.last() {
        Some(vctx.virtual_sheet_name.clone())
    } else if let Some(s) = selected_sheet_name_clone {
        Some(s.clone())
    } else {
        if in_structure_mode {
            if let Some(ref detail_ctx) = state.ai_structure_detail_context {
                state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
                state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
            }
        }
        None
    }
}

/// Builds the union of non-structure columns from reviews
pub fn build_union_columns(state: &EditorWindowState) -> Vec<usize> {
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
    union_cols
}

/// Builds merged column list based on review mode
pub fn build_merged_columns(
    state: &EditorWindowState,
    union_cols: &[usize],
    in_structure_mode: bool,
    in_virtual_structure_review: bool,
    active_sheet_name: &str,
    selected_category_clone: &Option<String>,
    registry: &SheetRegistry,
) -> (Vec<ColumnEntry>, Vec<crate::sheets::definitions::StructureFieldDefinition>) {
    if in_structure_mode {
        // In structure detail mode: build columns from structure schema
        build_structure_columns(
            union_cols,
            &state.ai_structure_detail_context,
            false,
            "",
            selected_category_clone,
            registry,
        )
    } else if in_virtual_structure_review {
        // In virtual structure review: filter out structure columns
        build_structure_columns(
            union_cols,
            &None,
            true,
            active_sheet_name,
            selected_category_clone,
            registry,
        )
    } else {
        // Normal mode: build columns from sheet metadata
        // IMPORTANT: Only show columns that are BOTH included in metadata AND present in union_cols (actually sent)
        let cols =
            if let Some(sheet) = registry.get_sheet(selected_category_clone, active_sheet_name) {
                if let Some(metadata) = &sheet.metadata {
                    let mut result = Vec::new();
                    for (col_idx, col_def) in metadata.columns.iter().enumerate() {
                        let is_structure =
                            matches!(col_def.validator, Some(ColumnValidator::Structure));

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
                    union_cols
                        .iter()
                        .map(|&idx| ColumnEntry::Regular(idx))
                        .collect()
                }
            } else {
                union_cols
                    .iter()
                    .map(|&idx| ColumnEntry::Regular(idx))
                    .collect()
            };
        (cols, Vec::new())
    }
}

/// Gathers ancestor key columns for display
pub fn gather_ancestor_key_columns(
    state: &EditorWindowState,
    in_structure_mode: bool,
    active_sheet_name: &str,
    selected_category_clone: &Option<String>,
    registry: &SheetRegistry,
) -> Vec<(String, String)> {
    let mut ancestor_key_columns: Vec<(String, String)> = Vec::new();

    if in_structure_mode {
        if let Some(ref detail_ctx) = state.ai_structure_detail_context {
            ancestor_key_columns = build_structure_ancestor_keys(
                detail_ctx,
                state,
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

    ancestor_key_columns
}

/// Updates pending merge and structure state flags
pub fn update_review_state_flags(state: &mut EditorWindowState) {
    state.ai_batch_has_undecided_merge = state
        .ai_new_row_reviews
        .iter()
        .any(|nr| nr.duplicate_match_row.is_some() && !nr.merge_decided);
}

/// Checks if there are undecided structures
pub fn has_undecided_structures(state: &EditorWindowState) -> bool {
    state
        .ai_structure_reviews
        .iter()
        .any(|entry| entry.is_undecided())
}

/// Processes accept all action in structure mode
pub fn process_accept_all_structure_mode(
    state: &mut EditorWindowState,
    detail_ctx: &StructureDetailContext,
) {
    // Persist respects user's cell-by-cell choices (Original vs AI)
    persist_structure_detail_changes(state, detail_ctx);
    if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| {
        match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
            (Some(a), Some(b)) if a == b => {
                sr.structure_path == detail_ctx.structure_path
            }
            (None, None) => {
                sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                    && sr.structure_path == detail_ctx.structure_path
            }
            _ => false,
        }
    }) {
        // Mark as accepted and decided, but don't override has_changes - it was calculated by persist_structure_detail_changes
        entry.accepted = true;
        entry.rejected = false;
        entry.decided = true;
    }
    // Restore top-level reviews
    state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
    state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
    state.ai_structure_detail_context = None; // back out
}

/// Processes accept all action in normal mode
pub fn process_accept_all_normal_mode(
    state: &mut EditorWindowState,
    selected_category_clone: &Option<String>,
    active_sheet_name: &str,
    cell_update_writer: &mut EventWriter<UpdateCellEvent>,
    add_row_writer: &mut EventWriter<AddSheetRowRequest>,
) {
    let existing_indices: Vec<usize> = (0..state.ai_row_reviews.len()).collect();
    let new_indices: Vec<usize> = state
        .ai_new_row_reviews
        .iter()
        .enumerate()
        .filter(|(_, nr)| nr.duplicate_match_row.is_none() || nr.merge_decided)
        .map(|(i, _)| i)
        .collect();
    process_existing_accept(
        &existing_indices,
        state,
        selected_category_clone,
        active_sheet_name,
        cell_update_writer,
    );
    process_new_accept(
        &new_indices,
        state,
        selected_category_clone,
        active_sheet_name,
        cell_update_writer,
        add_row_writer,
    );
    cancel_batch(state);
}

/// Processes decline all action in structure mode
pub fn process_decline_all_structure_mode(state: &mut EditorWindowState) {
    if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
        if let Some(entry) = state.ai_structure_reviews.iter_mut().find(|sr| {
            match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
                (Some(a), Some(b)) if a == b => {
                    sr.structure_path == detail_ctx.structure_path
                }
                (None, None) => {
                    sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                        && sr.structure_path == detail_ctx.structure_path
                }
                _ => false,
            }
        }) {
            entry.accepted = false;
            entry.rejected = true;
            entry.decided = true;
        }
        // Restore top-level reviews
        state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
        state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
    }
    state.ai_structure_detail_context = None;
}
