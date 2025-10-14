// src/sheets/systems/ai/results/row_processors.rs
// Row processing logic for AI batch results

use bevy::prelude::*;
use std::collections::HashMap;

use crate::sheets::events::AiBatchTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, NewRowReview, RowReview};

use crate::sheets::systems::ai::row_helpers::{
    create_row_snapshots, extract_ai_snapshot_from_new_row, generate_review_choices,
    normalize_cell_value, skip_key_prefix,
};

use super::duplicate_detection::check_for_duplicate;

/// Process original (existing) rows from batch result
pub fn process_original_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ev: &AiBatchTaskResult,
    orig_slice: &[Vec<String>],
) {
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    state.ai_structure_reviews.clear();

    let included = &ev.included_non_structure_columns;
    let (cat_ctx, sheet_ctx) = state.current_sheet_context();

    for (i, &row_index) in ev.original_row_indices.iter().enumerate() {
        let suggestion_full = &orig_slice[i];
        let suggestion = skip_key_prefix(suggestion_full, ev.key_prefix_count);

        if suggestion.len() < included.len() {
            warn!(
                "Skipping malformed original suggestion row {}: suggestion_cols={} < included_cols={} (full_len={}, key_prefix_count={})",
                row_index,
                suggestion.len(),
                included.len(),
                suggestion_full.len(),
                ev.key_prefix_count
            );
            continue;
        }

        let Some(sheet_name) = &sheet_ctx else {
            continue;
        };

        let (original_snapshot, ai_snapshot) = create_row_snapshots(
            registry, &cat_ctx, sheet_name, row_index, suggestion, included,
        );

        let choices = generate_review_choices(&original_snapshot, &ai_snapshot);

        state.ai_row_reviews.push(RowReview {
            row_index,
            original: original_snapshot,
            ai: ai_snapshot,
            choices,
            non_structure_columns: included.clone(),
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: Vec::new(),
        });

        // CACHE POPULATION: Store full grid row for rendering original previews
        // Includes raw structure JSON for on-demand parsing or lookup in StructureReviewEntry
        if let Some(sheet_name) = &sheet_ctx {
            if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                if let Some(full_row) = sheet_ref.grid.get(row_index) {
                    state
                        .ai_original_row_snapshot_cache
                        .insert((Some(row_index), None), full_row.clone());
                }
            }
        }
    }
}

/// Process new (AI-added) rows from batch result
pub fn process_new_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ev: &AiBatchTaskResult,
    extra_slice: &[Vec<String>],
) {
    let included = &ev.included_non_structure_columns;
    let (cat_ctx, sheet_ctx) = state.current_sheet_context();

    // Build duplicate detection map
    // Choose a key column to detect duplicates that is NOT the technical parent_key (col 1)
    // If all included columns are parent_key (unlikely), fall back to the first one.
    let key_actual_col_opt = included
        .iter()
        .copied()
        .find(|&c| c != 1)
        .or_else(|| included.first().copied());
    let mut first_col_value_to_row: HashMap<String, usize> = HashMap::new();
    if let Some(first_col_actual) = key_actual_col_opt {
        if let Some(sheet_name) = &sheet_ctx {
            if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
                    if let Some(val) = row.get(first_col_actual) {
                        let norm = normalize_cell_value(val);
                        if !norm.is_empty() {
                            first_col_value_to_row.entry(norm).or_insert(row_idx);
                        }
                    }
                }
            }
        }
    }

    for new_row_full in extra_slice.iter() {
        let new_row = skip_key_prefix(new_row_full, ev.key_prefix_count);

        if new_row.len() < included.len() {
            warn!(
                "Skipping malformed new suggestion row (cols {} < included_cols={} full_len={} key_prefix_count={})",
                new_row.len(),
                included.len(),
                new_row_full.len(),
                ev.key_prefix_count
            );
            continue;
        }

        let ai_snapshot = extract_ai_snapshot_from_new_row(new_row, included);

        let (duplicate_match_row, choices, original_for_merge, merge_selected) =
            check_for_duplicate(
                &ai_snapshot,
                &first_col_value_to_row,
                included,
                key_actual_col_opt,
                &cat_ctx,
                &sheet_ctx,
                registry,
            );

        let new_row_idx = state.ai_new_row_reviews.len();
        state.ai_new_row_reviews.push(NewRowReview {
            ai: ai_snapshot.clone(),
            non_structure_columns: included.clone(),
            duplicate_match_row,
            choices,
            merge_selected,
            merge_decided: false,
            original_for_merge: original_for_merge.clone(),
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: Vec::new(),
        });

        // CACHE POPULATION: Store snapshot for new/duplicate rows
        // - Duplicates: Use matched existing row (includes structure JSON)
        // - New rows: Empty snapshot (no original content)
        // This unifies preview rendering across all row types (existing/new/duplicate)
        if let Some(matched_idx) = duplicate_match_row {
            if let Some(sheet_name) = &sheet_ctx {
                if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                    if let Some(full_row) = sheet_ref.grid.get(matched_idx) {
                        state
                            .ai_original_row_snapshot_cache
                            .insert((None, Some(new_row_idx)), full_row.clone());
                    }
                }
            }
        } else {
            // Truly new rows (no duplicate): empty snapshot matching column count
            if let Some(sheet_name) = &sheet_ctx {
                if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                    if let Some(meta) = &sheet_ref.metadata {
                        let empty_row = vec![String::new(); meta.columns.len()];
                        state
                            .ai_original_row_snapshot_cache
                            .insert((None, Some(new_row_idx)), empty_row);
                    }
                }
            }
        }
    }
}
