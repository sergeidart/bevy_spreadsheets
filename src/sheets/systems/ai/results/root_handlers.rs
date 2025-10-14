// src/sheets/systems/ai/results/root_handlers.rs
// Root batch result handlers (Phase 1)

use bevy::prelude::*;

use crate::sheets::events::{AiBatchTaskResult, SheetOperationFeedback};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

use crate::sheets::systems::ai::phase2_helpers;

/// Handle root batch results - Phase 1: Initial discovery call
/// Detects duplicates and triggers Phase 2 deep review automatically
pub fn handle_root_batch_result_phase1(
    ev: &AiBatchTaskResult,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
    commands: &mut Commands,
    runtime: &bevy_tokio_tasks::TokioTasksRuntime,
    session_api_key: &crate::SessionApiKey,
) {
    match &ev.result {
        Ok(rows) => {
            let originals = ev.original_row_indices.len();
            info!(
                "PHASE 1: Received {} rows ({} originals, {} new)",
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
                    "Phase 1 complete - {} row(s) received, analyzing...",
                    rows.len()
                );
                state.add_ai_call_log(status, Some(raw.clone()), None, false);
            }

            // Detect duplicates in new rows
            let (_orig_slice, extra_slice) = if originals == 0 {
                (&[][..], &rows[..])
            } else {
                rows.split_at(originals)
            };

            let duplicate_indices = phase2_helpers::detect_duplicate_indices(
                extra_slice,
                &ev.included_non_structure_columns,
                ev.key_prefix_count,
                state,
                registry,
            );

            info!(
                "PHASE 1: Detected {} duplicates out of {} new rows",
                duplicate_indices.len(),
                extra_slice.len()
            );

            // Store Phase 1 intermediate data
            let (cat_ctx, sheet_ctx) = state.current_sheet_context();
            let sheet_name = sheet_ctx.unwrap_or_default();

            state.ai_phase1_intermediate =
                Some(crate::ui::elements::editor::state::Phase1IntermediateData {
                    all_ai_rows: rows.clone(),
                    duplicate_indices: duplicate_indices.clone(),
                    original_count: originals,
                    included_columns: ev.included_non_structure_columns.clone(),
                    category: cat_ctx.clone(),
                    sheet_name: sheet_name.clone(),
                    key_prefix_count: ev.key_prefix_count,
                    original_row_indices: ev.original_row_indices.clone(),
                });

            // OPTIMIZATION 1: Skip Phase 2 if only one column is being processed
            // With single column, there's no merge complexity, so use Phase 1 results directly
            let skip_phase2_single_column = ev.included_non_structure_columns.len() <= 1;

            // OPTIMIZATION 2: Skip Phase 2 if in structure sheet and no planned structure paths
            // When working within a real structure sheet directly, we don't need deep review
            // since there are no parent structures to include in the context
            let in_structure_sheet = !state.structure_navigation_stack.is_empty();
            let skip_phase2_structure_context =
                in_structure_sheet && state.ai_planned_structure_paths.is_empty();

            if skip_phase2_single_column || skip_phase2_structure_context {
                if skip_phase2_single_column {
                    info!(
                        "SINGLE-COLUMN OPTIMIZATION: Skipping Phase 2 (only {} column(s)), using Phase 1 results directly",
                        ev.included_non_structure_columns.len()
                    );
                }
                if skip_phase2_structure_context {
                    info!(
                        "STRUCTURE-CONTEXT OPTIMIZATION: Skipping Phase 2 (in structure view with no parent structures), using Phase 1 results directly"
                    );
                }

                // Phase 1 complete (and skipping Phase 2)
                state.ai_completed_tasks += 1;

                // Process Phase 1 results directly as final results
                let established_row_count = originals + duplicate_indices.len();
                phase2_helpers::handle_deep_review_result_phase2(
                    ev,
                    &duplicate_indices,
                    established_row_count,
                    state,
                    registry,
                    feedback_writer,
                );
            } else {
                // Phase 1 complete, will trigger Phase 2
                state.ai_completed_tasks += 1;
                // Update total to include Phase 2
                state.ai_total_tasks += 1;

                // Trigger Phase 2: Deep review call
                phase2_helpers::trigger_phase2_deep_review(
                    state,
                    registry,
                    commands,
                    runtime,
                    session_api_key,
                    rows,
                    &duplicate_indices,
                    originals,
                    &ev.included_non_structure_columns,
                    ev.key_prefix_count,
                );

                // Do NOT enqueue structure jobs yet - wait for Phase 2 to complete
                // Structure jobs will be enqueued in handle_deep_review_result_phase2
            }
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

    // Don't auto-open log panel - user will open manually if needed
    feedback_writer.write(SheetOperationFeedback {
        message: format!("AI batch error: {}", err),
        is_error: true,
    });
    state.ai_mode = crate::ui::elements::editor::state::AiModeState::Preparing;
}
