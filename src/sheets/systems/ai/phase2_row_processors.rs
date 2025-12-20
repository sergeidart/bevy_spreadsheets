// src/sheets/systems/ai/phase2_row_processors.rs
// Row processing - specialized processors for original, duplicate, and new rows
// NOTE: These functions are modular alternatives to inline processing in root_handlers.rs

#![allow(dead_code)]

use bevy::prelude::*;

use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, NewRowReview, RowReview};

use super::row_helpers::{
    create_row_snapshots, extract_ai_snapshot_from_new_row, generate_review_choices,
    skip_key_prefix,
};
use super::column_helpers::calculate_dynamic_prefix;
use super::duplicate_map_helpers::build_duplicate_map_for_parents;
use super::original_cache::cache_original_row_for_review;

/// Process original rows from batch results
/// 
/// These are existing rows that were included in the batch request.
/// They already exist in the database and will be shown in the RowReview UI.
pub fn process_original_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ctx: &crate::ui::elements::editor::state::BatchProcessingContext,
    orig_slice: &[Vec<String>],
) {
    let included = &ctx.included_columns;
    let cat_ctx = &ctx.category;
    let sheet_ctx = &ctx.sheet_name;

    for (i, suggestion_full) in orig_slice.iter().enumerate() {
        // Use actual original row index from context
        let row_index = ctx.original_row_indices.get(i).copied().unwrap_or(i);
        // Infer prefix count from inbound row: total_len - included_len
        let dynamic_prefix = calculate_dynamic_prefix(suggestion_full.len(), included.len());
        let parent_prefix_values: Vec<String> = suggestion_full
            .iter()
            .take(dynamic_prefix)
            .cloned()
            .collect();
        let suggestion = skip_key_prefix(suggestion_full, dynamic_prefix);

        if suggestion.len() < included.len() {
            warn!("Skipping malformed original row suggestion");
            continue;
        }

        let (original_snapshot, ai_snapshot) = create_row_snapshots(
            registry, cat_ctx, sheet_ctx, row_index, suggestion, included,
        );

        let choices = generate_review_choices(&original_snapshot, &ai_snapshot);

        state.ai_row_reviews.push(RowReview {
            row_index,
            original: original_snapshot,
            ai: ai_snapshot,
            choices,
            non_structure_columns: included.clone(),
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: parent_prefix_values.clone(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
            is_orphan: false,
        });

        // Cache original row using helper
        cache_original_row_for_review(
            state,
            registry,
            cat_ctx,
            &Some(sheet_ctx.clone()),
            Some(row_index),
            None,
        );
    }
}

/// Process duplicate rows as merge candidates
/// 
/// These are AI-suggested rows that match existing rows in the database.
/// They will be shown in the NewRowReview UI with merge options.
/// duplicate_match_row contains the row_index of the matched original.
pub fn process_duplicate_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ctx: &crate::ui::elements::editor::state::BatchProcessingContext,
    dup_slice: &[Vec<String>],
    _duplicate_indices: &[usize],
) {
    let included = &ctx.included_columns;
    let cat_ctx = &ctx.category;
    let sheet_ctx = &ctx.sheet_name;

    // Choose a key column that isn't the technical parent_key (1)
    let key_actual_col_opt = included
        .iter()
        .copied()
        .find(|&c| c != 1)
        .or_else(|| included.first().copied());
    let sheet_ctx_opt = Some(sheet_ctx.clone());

    for suggestion_full in dup_slice.iter() {
        let dynamic_prefix = calculate_dynamic_prefix(suggestion_full.len(), included.len());
        let parent_prefix_values: Vec<String> = suggestion_full
            .iter()
            .take(dynamic_prefix)
            .cloned()
            .collect();
        let suggestion = skip_key_prefix(suggestion_full, dynamic_prefix);

        // Build duplicate map per row with proper parent context
        // Base level: empty parent_prefix_values -> checks only first data column
        // Child level: non-empty parent_prefix_values -> checks first data column AND parent_key
        let first_col_value_to_row = build_duplicate_map_for_parents(
            &parent_prefix_values,
            key_actual_col_opt,
            cat_ctx,
            &sheet_ctx_opt,
            registry,
            state,
            included,
        );

        if suggestion.len() < included.len() {
            continue;
        }

        let ai_snapshot = extract_ai_snapshot_from_new_row(suggestion, included);

        // Find the matched existing row (reuse key_actual_col_opt calculated earlier)
        let (duplicate_match_row, choices, original_for_merge, merge_selected) =
            super::results::check_for_duplicate(
                &ai_snapshot,
                &first_col_value_to_row,
                included,
                key_actual_col_opt,
                cat_ctx,
                &sheet_ctx_opt,
                registry,
            );

        // For duplicates, projected_row_index is the matched original's row_index
        // until the user decides to keep as new (then it gets a fresh projected index)
        let projected_row_index = duplicate_match_row.unwrap_or(0);

        state.ai_new_row_reviews.push(NewRowReview {
            ai: ai_snapshot.clone(),
            non_structure_columns: included.clone(),
            duplicate_match_row,
            choices,
            merge_selected,
            merge_decided: false,
            original_for_merge: original_for_merge.clone(),
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: parent_prefix_values.clone(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
            projected_row_index,
            is_orphan: false,
        });

        // Cache original for duplicate using helper
        let new_row_idx = state.ai_new_row_reviews.len() - 1;
        cache_original_row_for_review(
            state,
            registry,
            cat_ctx,
            &sheet_ctx_opt,
            duplicate_match_row,
            Some(new_row_idx),
        );
    }
}

/// Process new AI-added rows (these are genuinely new suggestions)
/// 
/// These are genuinely new rows suggested by AI that don't match existing rows.
/// They will be shown in the NewRowReview UI for approval.
/// projected_row_index is assigned based on max existing + 1 + position.
pub fn process_new_rows(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    ctx: &crate::ui::elements::editor::state::BatchProcessingContext,
    new_slice: &[Vec<String>],
    max_row_index: usize,
    new_start_offset: usize,
) {
    let included = &ctx.included_columns;
    let cat_ctx = &ctx.category;
    let sheet_ctx = &ctx.sheet_name;

    for (pos, suggestion_full) in new_slice.iter().enumerate() {
        let dynamic_prefix = calculate_dynamic_prefix(suggestion_full.len(), included.len());
        let parent_prefix_values: Vec<String> = suggestion_full
            .iter()
            .take(dynamic_prefix)
            .cloned()
            .collect();
        let suggestion = skip_key_prefix(suggestion_full, dynamic_prefix);

        if suggestion.len() < included.len() {
            continue;
        }

        let ai_snapshot = extract_ai_snapshot_from_new_row(suggestion, included);

        // Calculate projected row index for this new row
        let projected_row_index = max_row_index + 1 + new_start_offset + pos;

        state.ai_new_row_reviews.push(NewRowReview {
            ai: ai_snapshot,
            non_structure_columns: included.clone(),
            duplicate_match_row: None,
            choices: None,
            merge_selected: false,
            merge_decided: false,
            original_for_merge: None,
            key_overrides: std::collections::HashMap::new(),
            ancestor_key_values: parent_prefix_values.clone(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
            projected_row_index,
            is_orphan: false,
        });

        // Cache empty original for new rows using helper
        let new_row_idx = state.ai_new_row_reviews.len() - 1;
        cache_original_row_for_review(
            state,
            registry,
            cat_ctx,
            &Some(sheet_ctx.clone()),
            None,
            Some(new_row_idx),
        );
    }
}
