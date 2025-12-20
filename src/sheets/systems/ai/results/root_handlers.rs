// src/sheets/systems/ai/results/root_handlers.rs
// Root batch result handlers - single-phase iterative processing

use bevy::prelude::*;

use crate::sheets::events::{AiBatchTaskResult, SheetOperationFeedback};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{
    BatchProcessingContext, EditorWindowState, NewRowReview, RowReview,
};

use crate::sheets::systems::ai::row_helpers::{
    create_row_snapshots, extract_ai_snapshot_from_new_row, generate_review_choices,
    skip_key_prefix,
};
use crate::sheets::systems::ai::column_helpers::calculate_dynamic_prefix;
use crate::sheets::systems::ai::duplicate_map_helpers::build_duplicate_map_for_parents;
use crate::sheets::systems::ai::original_cache::cache_original_row_for_review;
use crate::sheets::systems::ai::phase2_helpers::detect_duplicate_indices;
use crate::sheets::systems::ai::structure_jobs::enqueue_structure_jobs_for_batch;

/// Handle root batch results - single-phase processing
/// 
/// Processes AI results in one pass:
/// 1. Detect duplicates in new rows (use original indices for duplicates)
/// 2. Process original rows → RowReview
/// 3. Process duplicate rows → NewRowReview with projected_row_index = matched original
/// 4. Process new rows → NewRowReview with projected_row_index = max + sequence
/// 5. Enqueue structure jobs for child tables
#[allow(clippy::too_many_arguments)]
pub fn handle_batch_result(
    ev: &AiBatchTaskResult,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
    _commands: &mut Commands,
    _runtime: &bevy_tokio_tasks::TokioTasksRuntime,
    _session_api_key: &crate::SessionApiKey,
) {
    match &ev.result {
        Ok(rows) => {
            let originals = ev.original_row_indices.len();
            info!(
                "AI Batch: Received {} rows ({} originals, {} new)",
                rows.len(),
                originals,
                rows.len().saturating_sub(originals)
            );

            if originals > 0 && rows.len() < originals {
                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "AI batch result malformed: returned {} rows but expected at least {}",
                        rows.len(),
                        originals
                    ),
                    is_error: true,
                });
                return;
            }

            if let Some(raw) = &ev.raw_response {
                state.ai_raw_output_display = raw.clone();
                let status = format!(
                    "Processing {} row(s)...",
                    rows.len()
                );
                state.add_ai_call_log(status, Some(raw.clone()), None, false);
            }

            // Split into original and new rows
            let (orig_slice, extra_slice) = if originals == 0 {
                (&[][..], &rows[..])
            } else {
                rows.split_at(originals)
            };

            // Detect duplicates in new rows BEFORE processing
            let duplicate_indices = detect_duplicate_indices(
                extra_slice,
                &ev.included_non_structure_columns,
                ev.key_prefix_count,
                state,
                registry,
            );

            info!(
                "AI Batch: Detected {} duplicates out of {} new rows",
                duplicate_indices.len(),
                extra_slice.len()
            );

            // Get context for processing
            let (cat_ctx, sheet_ctx) = state.current_sheet_context();
            let sheet_name = sheet_ctx.clone().unwrap_or_default();
            let included = &ev.included_non_structure_columns;

            // Setup context prefixes
            super::setup_context_prefixes(state, registry, ev);

            // Clear previous reviews before populating
            state.ai_row_reviews.clear();
            state.ai_new_row_reviews.clear();
            state.ai_structure_reviews.clear();

            // Calculate max existing row_index for projected index assignment
            let max_existing_row_index = ev.original_row_indices.iter().copied().max().unwrap_or(0);
            let mut next_projected_index = max_existing_row_index + 1;

            // Process original rows → RowReview
            for (i, suggestion_full) in orig_slice.iter().enumerate() {
                let row_index = ev.original_row_indices.get(i).copied().unwrap_or(i);
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
                    registry, &cat_ctx, &sheet_name, row_index, suggestion, included,
                );

                let choices = generate_review_choices(&original_snapshot, &ai_snapshot);

                state.ai_row_reviews.push(RowReview {
                    row_index,
                    original: original_snapshot,
                    ai: ai_snapshot,
                    choices,
                    non_structure_columns: included.clone(),
                    key_overrides: std::collections::HashMap::new(),
                    ancestor_key_values: parent_prefix_values,
                    ancestor_dropdown_cache: std::collections::HashMap::new(),
                    is_orphan: false,
                });

                cache_original_row_for_review(
                    state,
                    registry,
                    &cat_ctx,
                    &Some(sheet_name.clone()),
                    Some(row_index),
                    None,
                );
            }

            // Build duplicate map for new row processing
            let key_actual_col_opt = included
                .iter()
                .copied()
                .find(|&c| c != 1)
                .or_else(|| included.first().copied());
            let sheet_ctx_opt = Some(sheet_name.clone());

            // Process new rows - split into duplicates and genuinely new
            for (new_idx, suggestion_full) in extra_slice.iter().enumerate() {
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

                let is_duplicate = duplicate_indices.contains(&new_idx);

                if is_duplicate {
                    // Duplicate row - find the matched original and use its row_index
                    let first_col_value_to_row = build_duplicate_map_for_parents(
                        &parent_prefix_values,
                        key_actual_col_opt,
                        &cat_ctx,
                        &sheet_ctx_opt,
                        registry,
                        state,
                        included,
                    );

                    let (duplicate_match_row, choices, original_for_merge, merge_selected) =
                        super::check_for_duplicate(
                            &ai_snapshot,
                            &first_col_value_to_row,
                            included,
                            key_actual_col_opt,
                            &cat_ctx,
                            &sheet_ctx_opt,
                            registry,
                        );

                    // For duplicates, projected_row_index = matched original's row_index
                    let projected_row_index = duplicate_match_row.unwrap_or(next_projected_index);

                    state.ai_new_row_reviews.push(NewRowReview {
                        ai: ai_snapshot,
                        non_structure_columns: included.clone(),
                        duplicate_match_row,
                        choices,
                        merge_selected,
                        merge_decided: false,
                        original_for_merge,
                        key_overrides: std::collections::HashMap::new(),
                        ancestor_key_values: parent_prefix_values,
                        ancestor_dropdown_cache: std::collections::HashMap::new(),
                        projected_row_index,
                        is_orphan: false,
                    });

                    let new_row_idx = state.ai_new_row_reviews.len() - 1;
                    cache_original_row_for_review(
                        state,
                        registry,
                        &cat_ctx,
                        &sheet_ctx_opt,
                        duplicate_match_row,
                        Some(new_row_idx),
                    );
                } else {
                    // Genuinely new row - assign next projected index
                    let projected_row_index = next_projected_index;
                    next_projected_index += 1;

                    state.ai_new_row_reviews.push(NewRowReview {
                        ai: ai_snapshot,
                        non_structure_columns: included.clone(),
                        duplicate_match_row: None,
                        choices: None,
                        merge_selected: false,
                        merge_decided: false,
                        original_for_merge: None,
                        key_overrides: std::collections::HashMap::new(),
                        ancestor_key_values: parent_prefix_values,
                        ancestor_dropdown_cache: std::collections::HashMap::new(),
                        projected_row_index,
                        is_orphan: false,
                    });

                    let new_row_idx = state.ai_new_row_reviews.len() - 1;
                    cache_original_row_for_review(
                        state,
                        registry,
                        &cat_ctx,
                        &sheet_ctx_opt,
                        None,
                        Some(new_row_idx),
                    );
                }
            }

            // Store batch context for structure job enqueueing
            let batch_context = BatchProcessingContext {
                duplicate_indices: duplicate_indices.clone(),
                original_count: originals,
                included_columns: included.clone(),
                category: cat_ctx.clone(),
                sheet_name: sheet_name.clone(),
                original_row_indices: ev.original_row_indices.clone(),
            };

            // Restore original row indices for structure processing
            state.ai_last_send_root_rows = ev.original_row_indices.clone();

            // Enqueue structure jobs (pass batch_context before storing in state)
            let expected_structure_jobs = enqueue_structure_jobs_for_batch(
                state,
                registry,
                Some(&batch_context),
            );

            // Store batch context after using it
            state.ai_batch_context = Some(batch_context);

            // Mark task complete
            state.ai_completed_tasks += 1;

            state.ai_batch_has_undecided_merge = state
                .ai_new_row_reviews
                .iter()
                .any(|nr| nr.duplicate_match_row.is_some() && !nr.merge_decided);

            state.ai_mode = crate::ui::elements::editor::state::AiModeState::ResultsReady;
            state.refresh_structure_waiting_state();

            if let Some(_raw) = &ev.raw_response {
                let status = format!(
                    "Complete - {} original(s), {} new row(s), {} structure job(s)",
                    state.ai_row_reviews.len(),
                    state.ai_new_row_reviews.len(),
                    expected_structure_jobs
                );
                // Don't include raw response here - already logged in "Processing..." entry
                state.add_ai_call_log(status, None, None, false);
            }

            info!(
                "AI Batch Complete: {} originals, {} new rows, {} structures",
                state.ai_row_reviews.len(),
                state.ai_new_row_reviews.len(),
                expected_structure_jobs
            );
        }
        Err(err) => {
            handle_root_batch_error(state, ev, err, feedback_writer);
        }
    }
}

/// Handle root batch errors
pub fn handle_root_batch_error(
    state: &mut EditorWindowState,
    ev: &AiBatchTaskResult,
    err: &str,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
) {
    if let Some(raw) = &ev.raw_response {
        state.ai_raw_output_display =
            format!("Batch Error: {}\n--- Raw Model Output ---\n{}", err, raw);
        state.add_ai_call_log(format!("Error: {}", err), Some(raw.clone()), None, true);
    } else {
        state.ai_raw_output_display = format!("Batch Error: {} (no raw output returned)", err);
        state.add_ai_call_log(format!("Error: {}", err), None, None, true);
    }

    feedback_writer.write(SheetOperationFeedback {
        message: format!("AI batch error: {}", err),
        is_error: true,
    });
    state.ai_mode = crate::ui::elements::editor::state::AiModeState::Preparing;
}
