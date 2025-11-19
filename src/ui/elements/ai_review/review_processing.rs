// src/ui/elements/ai_review/review_processing.rs
//! Helper functions for processing AI review data and actions

use crate::sheets::definitions::{ColumnValidator, SheetMetadata};
use crate::sheets::resources::SheetRegistry;
use crate::sheets::systems::ai_review::display_context::ReviewDisplayContext;
use crate::sheets::systems::ai_review::review_logic::ColumnEntry;
use crate::sheets::systems::ai_review::structure_persistence::{
    cleanup_declined_new_rows, structure_row_apply_existing, structure_row_apply_new,
};
use crate::sheets::systems::ai_review::{
    prepare_ai_suggested_plan, prepare_original_preview_plan, AiSuggestedPlan, OriginalPreviewPlan,
    RowBlock,
};
use crate::ui::elements::editor::state::{EditorWindowState, StructureReviewEntry};
use crate::ui::widgets::linked_column_cache::{self, CacheResult};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Populate linked column options for the current view
pub fn populate_linked_column_options(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    display_ctx: &ReviewDisplayContext,
    sheet_metadata: Option<&SheetMetadata>,
) -> HashMap<usize, Arc<HashSet<String>>> {
    let mut linked_column_options = HashMap::new();

    // For nested structures, use structure_schema; otherwise use sheet metadata columns
    if display_ctx.in_structure_mode && !display_ctx.structure_schema.is_empty() {
        // In structure detail mode: get validators from structure schema
        for col_entry in &display_ctx.merged_columns {
            if let ColumnEntry::Regular(actual_col) = col_entry {
                if let Some(field_def) = display_ctx.structure_schema.get(*actual_col) {
                    if let Some(ColumnValidator::Linked {
                        target_sheet_name,
                        target_column_index,
                    }) = &field_def.validator
                    {
                        if let CacheResult::Success { raw, .. } =
                            linked_column_cache::get_or_populate_linked_options(
                                &target_sheet_name,
                                *target_column_index,
                                registry,
                                state,
                            )
                        {
                            linked_column_options.insert(*actual_col, raw);
                        }
                    }
                }
            }
        }
    } else if let Some(meta) = sheet_metadata {
        // Normal mode or virtual structure review: get validators from sheet metadata
        for col_entry in &display_ctx.merged_columns {
            if let ColumnEntry::Regular(actual_col) = col_entry {
                if let Some(col_def) = meta.columns.get(*actual_col) {
                    if let Some(ColumnValidator::Linked {
                        target_sheet_name,
                        target_column_index,
                    }) = &col_def.validator
                    {
                        if let CacheResult::Success { raw, .. } =
                            linked_column_cache::get_or_populate_linked_options(
                                &target_sheet_name,
                                *target_column_index,
                                registry,
                                state,
                            )
                        {
                            linked_column_options.insert(*actual_col, raw);
                        }
                    }
                }
            }
        }
    }

    linked_column_options
}

/// Pre-compute AI and original plans for rendering
pub fn precompute_plans(
    state: &EditorWindowState,
    ai_structure_reviews: &[StructureReviewEntry],
    display_ctx: &ReviewDisplayContext,
    blocks: &[RowBlock],
    linked_column_options: &HashMap<usize, Arc<HashSet<String>>>,
) -> (
    HashMap<(usize, crate::sheets::systems::ai_review::RowKind), AiSuggestedPlan>,
    HashMap<(usize, crate::sheets::systems::ai_review::RowKind), OriginalPreviewPlan>,
) {
    // For navigation drill-down, pass a fake context to signal drill-down mode
    // For old structure detail mode, use the actual context
    let detail_ctx_for_plans = if !state.ai_navigation_stack.is_empty() {
        // In navigation drill-down: use a minimal context just to signal we're in drill-down
        Some(crate::ui::elements::editor::state::StructureDetailContext {
            root_category: state.ai_current_category.clone(),
            root_sheet: state.ai_current_sheet.clone(),
            parent_row_index: None,
            parent_new_row_index: None,
            structure_path: Vec::new(),
            hydrated: false,
            saved_row_reviews: Vec::new(),
            saved_new_row_reviews: Vec::new(),
        })
    } else {
        state.ai_structure_detail_context.clone()
    };
    let detail_ctx = detail_ctx_for_plans.as_ref();
    let mut ai_plans_cache = HashMap::new();
    let mut original_plans_cache = HashMap::new();

    for block in blocks {
        match block {
            RowBlock::AiSuggested(data_idx, kind) => {
                if !ai_plans_cache.contains_key(&(*data_idx, *kind)) {
                    if let Some(plan) = prepare_ai_suggested_plan(
                        state,
                        ai_structure_reviews,
                        detail_ctx,
                        &display_ctx.merged_columns,
                        linked_column_options,
                        *kind,
                        *data_idx,
                    ) {
                        ai_plans_cache.insert((*data_idx, *kind), plan);
                    }
                }
            }
            RowBlock::OriginalPreview(data_idx, kind) => {
                if !original_plans_cache.contains_key(&(*data_idx, *kind)) {
                    if let Some(plan) = prepare_original_preview_plan(
                        state,
                        ai_structure_reviews,
                        detail_ctx,
                        &display_ctx.merged_columns,
                        *kind,
                        *data_idx,
                    ) {
                        original_plans_cache.insert((*data_idx, *kind), plan);
                    }
                }
            }
            _ => {}
        }
    }

    (ai_plans_cache, original_plans_cache)
}

/// Process changes to structure reviews based on user actions
pub fn process_structure_review_changes(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    existing_accept: &[usize],
    existing_cancel: &[usize],
    new_accept: &[usize],
    new_cancel: &[usize],
) {
    // Find the structure review entry - handle both navigation drilldown and structure detail modes
    let entry_index = if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
        // Structure detail mode - use detail context
        state.ai_structure_reviews.iter().position(|sr| {
            match (sr.parent_new_row_index, detail_ctx.parent_new_row_index) {
                (Some(a), Some(b)) if a == b => sr.structure_path == detail_ctx.structure_path,
                (None, None) => {
                    sr.parent_row_index == detail_ctx.parent_row_index.unwrap_or(usize::MAX)
                        && sr.structure_path == detail_ctx.structure_path
                }
                _ => false,
            }
        })
    } else if !state.ai_navigation_stack.is_empty() {
        // Navigation drilldown mode - use parent filter and navigation stack
        let parent_ctx = state.ai_navigation_stack.last();
        let parent_review_index = parent_ctx.and_then(|ctx| ctx.parent_review_index);

        if let (Some(parent_ctx), Some(parent_review_idx)) = (parent_ctx, parent_review_index) {
            state.ai_structure_reviews.iter().position(|sr| {
                sr.root_category == parent_ctx.category
                    && sr.root_sheet == parent_ctx.sheet_name
                    && sr.parent_row_index == parent_review_idx
            })
        } else {
            None
        }
    } else {
        None
    };

    if let Some(entry_index) = entry_index {
        if !new_accept.is_empty()
            || !new_cancel.is_empty()
            || !existing_accept.is_empty()
            || !existing_cancel.is_empty()
        {
            info!("Found entry_index: {}", entry_index);
        }
        let entry_ptr: *mut _ = &mut state.ai_structure_reviews[entry_index];
        // Safe because we don't move state.ai_structure_reviews while using entry_ptr
        unsafe {
            let entry = &mut *entry_ptr;
            // Existing accepts
            if !existing_accept.is_empty() {
                info!(
                    "Child table: Processing {} existing accepts",
                    existing_accept.len()
                );
            }
            for &idx in existing_accept {
                if let Some(rr) = state.ai_row_reviews.get(idx) {
                    info!("Child table: Applying accept for existing row {}", idx);
                    structure_row_apply_existing(entry, rr, true);
                }
            }
            if !existing_cancel.is_empty() {
                info!(
                    "Child table: Processing {} existing cancels",
                    existing_cancel.len()
                );
            }
            for &idx in existing_cancel {
                if let Some(rr) = state.ai_row_reviews.get(idx) {
                    info!("Child table: Applying decline for existing row {}", idx);
                    structure_row_apply_existing(entry, rr, false);
                }
            }
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
            if !new_accept.is_empty() {
                info!("Child table: Processing {} new accepts", new_accept.len());
            }
            for &idx in new_accept {
                info!("Child table: Applying accept for new row {}", idx);
                structure_row_apply_new(entry, idx, &state.ai_new_row_reviews, true);
            }
            if !new_cancel.is_empty() {
                info!("Child table: Processing {} new cancels", new_cancel.len());
            }
            for &idx in new_cancel {
                info!("Child table: Applying decline for new row {}", idx);
                structure_row_apply_new(entry, idx, &state.ai_new_row_reviews, false);
            }
            // Remove accepted/declined new rows from temp view to mimic top-level behavior
            if !new_accept.is_empty() || !new_cancel.is_empty() {
                let mut to_remove: Vec<usize> = Vec::new();
                to_remove.extend(new_accept.iter().cloned());
                to_remove.extend(new_cancel.iter().cloned());
                to_remove.sort_unstable();
                to_remove.dedup();
                for idx in to_remove.into_iter().rev() {
                    if idx < state.ai_new_row_reviews.len() {
                        state.ai_new_row_reviews.remove(idx);
                    }
                }

                // Clean up declined rows from entry arrays now that processing is complete
                cleanup_declined_new_rows(entry);
            }
            // Mark entry has changes
            entry.has_changes = true;
            // Auto-mark decided and accepted if no remaining temp rows left
            let no_temp_rows =
                state.ai_row_reviews.is_empty() && state.ai_new_row_reviews.is_empty();
            if no_temp_rows && !entry.decided {
                entry.decided = true;
                if entry
                    .differences
                    .iter()
                    .all(|row| row.iter().all(|f| !*f))
                {
                    entry.accepted = true;
                    entry.rejected = false;
                }

                // Exit structure detail mode ONLY if using old detail context system
                // For navigation drilldown, automatically navigate back when all rows decided
                if let Some(ref detail_ctx) = state.ai_structure_detail_context.clone() {
                    state.ai_row_reviews = detail_ctx.saved_row_reviews.clone();
                    state.ai_new_row_reviews = detail_ctx.saved_new_row_reviews.clone();
                    state.ai_structure_detail_context = None;
                } else if !state.ai_navigation_stack.is_empty() {
                    info!("All child rows decided in navigation mode - automatically navigating back to parent");
                    // Trigger automatic navigation back to parent level
                    use crate::ui::elements::ai_review::navigation;
                    navigation::navigate_back(state, registry);
                }
            }
        }
    } else if !new_accept.is_empty()
        || !new_cancel.is_empty()
        || !existing_accept.is_empty()
        || !existing_cancel.is_empty()
    {
        info!("ERROR: Could not find matching entry in ai_structure_reviews!");
        if let Some(parent_ctx) = state.ai_navigation_stack.last() {
            info!("Navigation mode: parent_category={:?}, parent_sheet={}, parent_row={:?}, parent_review_index={:?}", 
                                        parent_ctx.category, parent_ctx.sheet_name, parent_ctx.parent_row_index, parent_ctx.parent_review_index);
        }
        info!(
            "Structure reviews count: {}",
            state.ai_structure_reviews.len()
        );
        for (i, sr) in state.ai_structure_reviews.iter().enumerate() {
            info!("  Entry {}: root_category={:?}, root_sheet={}, parent_row={}, parent_new_row={:?}", 
                                        i, sr.root_category, sr.root_sheet, sr.parent_row_index, sr.parent_new_row_index);
        }
    }
}
