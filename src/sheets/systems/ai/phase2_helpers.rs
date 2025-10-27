// src/sheets/systems/ai/phase2_helpers.rs
// Phase 2 deep review processing - handles duplicate detection and deep review workflow

use bevy::prelude::*;
use std::collections::HashMap;

use crate::sheets::events::{AiBatchTaskResult, SheetOperationFeedback};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{EditorWindowState, NewRowReview};

use super::row_helpers::{
    create_row_snapshots, extract_ai_snapshot_from_new_row, generate_review_choices,
    normalize_cell_value, skip_key_prefix,
};
use super::column_helpers::calculate_dynamic_prefix;
use super::duplicate_map_helpers::build_composite_duplicate_map_for_parents;
use super::structure_jobs::enqueue_structure_jobs_for_batch;

/// Detect which new rows are duplicates of existing rows (by first column)
pub fn detect_duplicate_indices(
    extra_slice: &[Vec<String>],
    included: &[usize],
    _key_prefix_count: usize,
    state: &EditorWindowState,
    registry: &SheetRegistry,
) -> Vec<usize> {
    let mut duplicate_indices = Vec::new();
    let (cat_ctx, sheet_ctx) = state.current_sheet_context();

    info!(
        "detect_duplicate_indices: extra_slice.len()={}, included={:?}, sheet_ctx={:?}",
        extra_slice.len(),
        included,
        sheet_ctx
    );

    // Choose a key column that isn't the technical parent_key (1)
    let key_actual_col_opt = included
        .iter()
        .copied()
        .find(|&c| c != 1)
        .or_else(|| included.first().copied());

    // Check each new row using a row-specific parent chain derived from inbound prefix values
    for (new_idx, new_row_full) in extra_slice.iter().enumerate() {
        // Infer per-row prefix count based on inbound row length
        let dynamic_prefix = calculate_dynamic_prefix(new_row_full.len(), included.len());
        let parent_prefix_values: Vec<String> = new_row_full
            .iter()
            .take(dynamic_prefix)
            .cloned()
            .collect();
        let new_row = skip_key_prefix(new_row_full, dynamic_prefix);

        if new_row.len() < included.len() {
            continue;
        }

        let ai_snapshot = extract_ai_snapshot_from_new_row(new_row, included);

        // Build a composite duplicate map for this row's parent chain
        // This map will use the same key format as we build below (without parent row indices in the key)
        let composite_map = build_composite_duplicate_map_for_parents(
            &parent_prefix_values,
            included,
            &cat_ctx,
            &sheet_ctx,
            registry,
            state,
        );

        if new_idx == 0 {
            info!("Composite map for parent {:?}: {:?}", parent_prefix_values, composite_map);
        }

        // Build composite key from AI values: just the data columns (normalized)
        // Note: parent filtering is done by the map builder, not by adding parent indices to the key
        let mut ai_composite_parts: Vec<String> = Vec::with_capacity(ai_snapshot.len());

        // Add AI data column values to the key (normalized)
        for ai_val in &ai_snapshot {
            ai_composite_parts.push(normalize_cell_value(ai_val));
        }

        let ai_composite = ai_composite_parts.join("||");

        info!(
            "Row {}: parent_prefix={:?}, ai_snapshot={:?}, ai_composite='{}', in_map={}",
            new_idx,
            parent_prefix_values,
            ai_snapshot,
            ai_composite,
            composite_map.contains_key(&ai_composite)
        );

        if composite_map.contains_key(&ai_composite) {
            duplicate_indices.push(new_idx);
        }
    }

    info!("Final duplicate_indices={:?}", duplicate_indices);
    duplicate_indices
}

/// Trigger Phase 2: Deep review call with all rows restructured
#[allow(clippy::too_many_arguments)]
pub fn trigger_phase2_deep_review(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    commands: &mut Commands,
    runtime: &bevy_tokio_tasks::TokioTasksRuntime,
    session_api_key: &crate::SessionApiKey,
    phase1_rows: &[Vec<String>],
    duplicate_indices: &[usize],
    original_count: usize,
    included: &[usize],
    key_prefix_count: usize,
) {
    use crate::sheets::systems::ai::control_handler::{spawn_batch_task, BatchPayload};

    let (cat_ctx, sheet_ctx) = state.current_sheet_context();
    let Some(sheet_name) = sheet_ctx else {
        return;
    };
    let Some(sheet) = registry.get_sheet(&cat_ctx, &sheet_name) else {
        return;
    };
    let Some(meta) = &sheet.metadata else {
        return;
    };

    // Build Phase 2 rows:
    // 1. Originals (full data)
    // 2. Duplicates (full data, will be marked for merge in UI)
    // 3. New AI-added (only first column)

    let mut phase2_rows: Vec<Vec<String>> = Vec::new();
    let (orig_slice, extra_slice) = phase1_rows.split_at(original_count);

    // 1. Add originals with full data
    for orig_row in orig_slice {
        phase2_rows.push(orig_row.clone());
    }

    // 2. Add duplicates with full data
    for &dup_idx in duplicate_indices {
        if let Some(dup_row) = extra_slice.get(dup_idx) {
            phase2_rows.push(dup_row.clone());
        }
    }

    // 3. Add new AI-added rows with only first column
    for (new_idx, new_row) in extra_slice.iter().enumerate() {
        if !duplicate_indices.contains(&new_idx) {
            // Only include first column value
            let mut minimal_row = vec![String::new(); new_row.len()];
            if !new_row.is_empty() {
                minimal_row[0] = new_row[0].clone();
            }
            phase2_rows.push(minimal_row);
        }
    }

    let established_row_count = original_count + duplicate_indices.len();

    info!(
        "PHASE 2 TRIGGER: Sending BASE LEVEL request with {} rows ({} originals + {} duplicates + {} new-minimal), allow_row_additions=false",
        phase2_rows.len(),
        original_count,
        duplicate_indices.len(),
        phase2_rows.len() - established_row_count
    );

    // Build column contexts
    let mut column_contexts: Vec<Option<String>> = Vec::new();
    for &col_idx in included {
        if let Some(col_def) = meta.columns.get(col_idx) {
            column_contexts.push(
                crate::ui::elements::ai_review::ai_context_utils::decorate_context_with_type(
                    col_def.ai_context.as_ref(),
                    col_def.data_type,
                ),
            );
        } else {
            column_contexts.push(None);
        }
    }

    let model_id = if meta.ai_model_id.is_empty() {
        crate::sheets::definitions::default_ai_model_id()
    } else {
        meta.ai_model_id.clone()
    };

    // Build payload with allow_row_additions: false
    let payload = BatchPayload {
        ai_model_id: model_id,
        general_sheet_rule: meta.ai_general_rule.clone(),
        column_contexts,
        rows_data: phase2_rows.clone(),
        requested_grounding_with_google_search: meta
            .requested_grounding_with_google_search
            .unwrap_or(false),
        allow_row_additions: false, // KEY: Phase 2 treats everything as existing
        // Do not include key_prefix_* metadata in payload
        key_prefix_count: None,
        key_prefix_headers: None,
        parent_groups: None,
        user_prompt: String::new(),
    };

    let payload_json = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            error!("Phase 2 payload serialization error: {}", e);
            return;
        }
    };

    // Pretty-print payload for logging
    let payload_pretty =
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload_json.clone());

    // Add log entry with request payload BEFORE sending
    state.add_ai_call_log(
        format!(
            "Phase 2: Sending BASE LEVEL deep review (allow_row_additions=false) - {} rows ({} originals + {} duplicates + {} new)",
            phase2_rows.len(),
            original_count,
            duplicate_indices.len(),
            phase2_rows.len() - established_row_count
        ),
        None,
        Some(payload_pretty),
        false,
    );

    // Set flag to route next result as Phase 2
    state.ai_expecting_phase2_result = true;

    info!("PHASE 2: Spawning batch task now...");
    spawn_batch_task(
        runtime,
        commands,
        session_api_key,
        payload_json,
        state.ai_last_send_root_rows.clone(),
        included.to_vec(),
        key_prefix_count,
    );
    info!("PHASE 2: Batch task spawned, waiting for result...");

    state.ai_mode = crate::ui::elements::editor::state::AiModeState::Submitting;
}

/// Handle Phase 2 deep review results
pub fn handle_deep_review_result_phase2(
    ev: &AiBatchTaskResult,
    duplicate_indices: &[usize],
    established_row_count: usize,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
) {
    match &ev.result {
        Ok(rows) => {
            info!(
                "PHASE 2: Received {} rows (established={}, duplicates={})",
                rows.len(),
                established_row_count,
                duplicate_indices.len()
            );

            // Get Phase 1 data
            let Some(phase1) = state.ai_phase1_intermediate.take() else {
                error!("Phase 2 result received but no Phase 1 data found!");
                return;
            };

            // Mark Phase 2 as complete if we were expecting it
            if state.ai_expecting_phase2_result {
                state.ai_completed_tasks += 1;
            }

            if let Some(raw) = &ev.raw_response {
                state.ai_raw_output_display = raw.clone();
                let status = format!("Phase 2 complete - {} row(s) processed", rows.len());
                state.add_ai_call_log(status, Some(raw.clone()), None, false);
            }

            // Now process the results properly
            super::results::setup_context_prefixes(state, registry, ev);

            // Clear previous reviews before populating from Phase 2
            state.ai_row_reviews.clear();
            state.ai_new_row_reviews.clear();
            state.ai_structure_reviews.clear();

            // Process originals
            let orig_slice = &rows[0..phase1.original_count];
            process_original_rows_from_phase2(state, registry, &phase1, orig_slice);

            // Process duplicates (marked as merge candidates)
            let dup_start = phase1.original_count;
            let dup_end = established_row_count;
            if dup_end > dup_start {
                let dup_slice = &rows[dup_start..dup_end];
                process_duplicate_rows_from_phase2(
                    state,
                    registry,
                    &phase1,
                    dup_slice,
                    duplicate_indices,
                );
            }

            // Process new AI-added rows
            if rows.len() > established_row_count {
                let new_slice = &rows[established_row_count..];
                process_new_rows_from_phase2(state, registry, &phase1, new_slice);
            }

            // Restore original row indices for structure processing
            state.ai_last_send_root_rows = phase1.original_row_indices.clone();

            // Enqueue structure jobs
            let expected_structure_jobs =
                enqueue_structure_jobs_for_batch(state, registry, Some(&phase1));

            state.ai_batch_has_undecided_merge = state
                .ai_new_row_reviews
                .iter()
                .any(|nr| nr.duplicate_match_row.is_some() && !nr.merge_decided);

            state.ai_mode = crate::ui::elements::editor::state::AiModeState::ResultsReady;
            state.refresh_structure_waiting_state();

            info!(
                "PHASE 2 COMPLETE: {} originals, {} duplicates, {} new, {} structures",
                phase1.original_count,
                duplicate_indices.len(),
                state.ai_new_row_reviews.len() - duplicate_indices.len(),
                expected_structure_jobs
            );
        }
        Err(err) => {
            error!("Phase 2 error: {}", err);
            state.ai_phase1_intermediate = None;
            super::results::handle_root_batch_error(state, ev, err, feedback_writer);
        }
    }
}

/// Process original rows from Phase 2 results
fn process_original_rows_from_phase2(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    phase1: &crate::ui::elements::editor::state::Phase1IntermediateData,
    orig_slice: &[Vec<String>],
) {
    use crate::ui::elements::editor::state::RowReview;

    // Same as before but using Phase 1 context
    let included = &phase1.included_columns;
    let cat_ctx = &phase1.category;
    let sheet_ctx = &phase1.sheet_name;

    for (i, suggestion_full) in orig_slice.iter().enumerate() {
        // Use actual original row index from Phase 1
        let row_index = phase1.original_row_indices.get(i).copied().unwrap_or(i);
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
        });

        // Cache original row
        if let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_ctx) {
            if let Some(full_row) = sheet_ref.grid.get(row_index) {
                state
                    .ai_original_row_snapshot_cache
                    .insert((Some(row_index), None), full_row.clone());
            }
        }
    }
}

/// Process duplicate rows from Phase 2 as merge candidates
fn process_duplicate_rows_from_phase2(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    phase1: &crate::ui::elements::editor::state::Phase1IntermediateData,
    dup_slice: &[Vec<String>],
    _duplicate_indices: &[usize],
) {
    let included = &phase1.included_columns;
    let cat_ctx = &phase1.category;
    let sheet_ctx = &phase1.sheet_name;

    // Build duplicate detection map to find matched rows
    let mut first_col_value_to_row: HashMap<String, usize> = HashMap::new();
    if let Some(first_col_actual) = included.first() {
        if let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_ctx) {
            for (row_idx, row) in sheet_ref.grid.iter().enumerate() {
                if let Some(val) = row.get(*first_col_actual) {
                    let norm = normalize_cell_value(val);
                    if !norm.is_empty() {
                        first_col_value_to_row.entry(norm).or_insert(row_idx);
                    }
                }
            }
        }
    }

    for suggestion_full in dup_slice.iter() {
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

        // Find the matched existing row
        let sheet_ctx_opt = Some(sheet_ctx.clone());
        // Choose a key column that isn't the technical parent_key (1)
        let key_actual_col_opt = included.iter().copied().find(|&c| c != 1).or_else(|| included.first().copied());

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

        // FIX: Cache original for duplicate using the new_row_index
        // This ensures structure detail view can find the original row
        let new_row_idx = state.ai_new_row_reviews.len() - 1;
        if let Some(matched_idx) = duplicate_match_row {
            if let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_ctx) {
                if let Some(full_row) = sheet_ref.grid.get(matched_idx) {
                    state
                        .ai_original_row_snapshot_cache
                        .insert((None, Some(new_row_idx)), full_row.clone());
                }
            }
        }
    }
}

/// Process new AI-added rows from Phase 2 (these had minimal data in Phase 2 request)
fn process_new_rows_from_phase2(
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    phase1: &crate::ui::elements::editor::state::Phase1IntermediateData,
    new_slice: &[Vec<String>],
) {
    let included = &phase1.included_columns;
    let cat_ctx = &phase1.category;
    let sheet_ctx = &phase1.sheet_name;

    for suggestion_full in new_slice.iter() {
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
        });

        // Cache empty original for new rows
        if let Some(sheet_ref) = registry.get_sheet(cat_ctx, sheet_ctx) {
            if let Some(meta) = &sheet_ref.metadata {
                let empty_row = vec![String::new(); meta.columns.len()];
                let new_row_idx = state.ai_new_row_reviews.len() - 1;
                state
                    .ai_original_row_snapshot_cache
                    .insert((None, Some(new_row_idx)), empty_row);
            }
        }
    }
}
