// src/sheets/systems/ai/results/row_processors.rs
// Row processing logic for AI batch results

use bevy::prelude::*;
use std::collections::HashMap;

use crate::sheets::events::AiBatchTaskResult;
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, NewRowReview, RowReview};

use crate::sheets::systems::ai::row_helpers::{
    create_row_snapshots, extract_ai_snapshot_from_new_row, extract_original_snapshot_for_merge,
    generate_review_choices, normalize_cell_value, skip_key_prefix,
};
use crate::sheets::systems::ai::parent_chain_helpers::{
    extract_parent_key_column, row_matches_parent_chain,
};
use crate::sheets::systems::ai::column_helpers::calculate_dynamic_prefix;
use crate::sheets::systems::ai::duplicate_map_helpers::build_composite_duplicate_map_for_parents;

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
        // Infer prefix count from inbound row: total_len - included_len
        let dynamic_prefix = calculate_dynamic_prefix(suggestion_full.len(), included.len());
        let parent_prefix_values: Vec<String> = suggestion_full
            .iter()
            .take(dynamic_prefix)
            .cloned()
            .collect();
        let suggestion = skip_key_prefix(suggestion_full, dynamic_prefix);

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
            ancestor_key_values: parent_prefix_values.clone(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
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

    // Build duplicate detection map constrained to the same ancestor chain when in a structure sheet
    // When AI sends rows with parent prefixes, we need to:
    // 1. Extract parent names from the prefix columns (human-readable)
    // 2. Convert parent names to row_index values
    // 3. Filter existing rows to only those matching ALL parent row_index values

    // Choose a key column to detect duplicates that is NOT a technical column; fallback to first included
    let key_actual_col_opt = included.iter().copied().find(|&c| c != 1).or_else(|| included.first().copied());
    let mut first_col_value_to_row: HashMap<String, usize> = HashMap::new();
    if let Some(first_col_actual) = key_actual_col_opt {
        if let Some(sheet_name) = &sheet_ctx {
            if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                let meta_opt = sheet_ref.metadata.as_ref();

                // Check if we're in a virtual structure (child table from JSON)
                let is_virtual_sheet = !state.virtual_structure_stack.is_empty()
                    && state.virtual_structure_stack.iter().any(|vctx| &vctx.virtual_sheet_name == sheet_name);

                // For virtual sheets: All rows belong to the same parent, so check against all
                // For real tables: Check parent_key columns if they exist
                if is_virtual_sheet {
                    // Virtual sheet - all rows already belong to same parent, check against all rows
                    for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
                        if let Some(val) = row.get(first_col_actual) {
                            let norm = normalize_cell_value(val);
                            if !norm.is_empty() {
                                first_col_value_to_row.entry(norm).or_insert(row_idx);
                            }
                        }
                    }
                } else {
                    // Real table (not virtual) - check parent_key columns to filter by ancestor chain
                    let has_ancestors = !state.virtual_structure_stack.is_empty();
                    let expected_parent_indices: Vec<usize> = if has_ancestors {
                        state.virtual_structure_stack.iter().map(|vctx| vctx.parent.parent_row).collect()
                    } else {
                        Vec::new()
                    };

                    let parent_key_col = if let Some(meta) = meta_opt {
                        extract_parent_key_column(meta)
                    } else { None };

                    for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
                        // If we have ancestors, only include rows matching ALL parent row_index values
                        if has_ancestors && !expected_parent_indices.is_empty() {
                            if !row_matches_parent_chain(row, &expected_parent_indices, parent_key_col) {
                                continue;
                            }
                        }

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
    }

    for new_row_full in extra_slice.iter() {
        // Extract parent prefix values (human-readable names like "Battlefield 2142")
        let dynamic_prefix = calculate_dynamic_prefix(new_row_full.len(), included.len());
        let parent_prefix_values: Vec<String> = new_row_full
            .iter()
            .take(dynamic_prefix)
            .cloned()
            .collect();

        info!(
            "Processing new AI row: key_prefix_count={}, parent_prefix_values={:?}, full_row={:?}",
            ev.key_prefix_count, parent_prefix_values, new_row_full
        );

        let new_row = skip_key_prefix(new_row_full, dynamic_prefix);

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

        // Build composite duplicate map for this row's parent chain
        // This map uses the same key format as we build below (without parent row indices)
        let composite_map = build_composite_duplicate_map_for_parents(
            &parent_prefix_values,
            included,
            &cat_ctx,
            &sheet_ctx,
            registry,
            state,
        );

        // Build composite key from AI values: just the data columns (normalized)
        // Note: parent filtering is done by the map builder, not by adding parent indices to the key
        let mut ai_composite_parts: Vec<String> = Vec::with_capacity(ai_snapshot.len());

        // Add AI data column values to the key (normalized)
        for ai_val in &ai_snapshot {
            ai_composite_parts.push(normalize_cell_value(ai_val));
        }

        let ai_composite = ai_composite_parts.join("||");

        let duplicate_match_row = composite_map.get(&ai_composite).copied();
        let (choices, original_for_merge, merge_selected) = if let Some(matched_idx) = duplicate_match_row {
            // Extract original snapshot and build choices
            if let Some(sheet_name) = &sheet_ctx {
                if let Some(sheet_ref) = registry.get_sheet(&cat_ctx, sheet_name) {
                    if let Some(existing_row) = sheet_ref.grid.get(matched_idx) {
                        let orig_vec = extract_original_snapshot_for_merge(existing_row, included);
                        let choices = generate_review_choices(&orig_vec, &ai_snapshot);
                        (Some(choices), Some(orig_vec), true)
                    } else {
                        (None, None, true)
                    }
                } else {
                    (None, None, true)
                }
            } else {
                (None, None, true)
            }
        } else {
            (None, None, false)
        };

        info!(
            "Duplicate check (composite) result: duplicate_match_row={:?}",
            duplicate_match_row,
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
            ancestor_key_values: parent_prefix_values.clone(),
            ancestor_dropdown_cache: std::collections::HashMap::new(),
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

