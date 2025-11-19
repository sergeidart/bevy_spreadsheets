// src/sheets/systems/ai/results/root_handlers.rs
// Root batch result handlers (Phase 1)

use bevy::prelude::*;

use crate::sheets::events::{AiBatchTaskResult, SheetOperationFeedback};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::EditorWindowState;

use crate::sheets::systems::ai::phase2_helpers;

/// Handle root batch results - Phase 1: Initial discovery call
/// Detects duplicates and triggers Phase 2 deep review automatically
///
/// # Phase Logic based on Layers
///
/// The two-phase logic applies differently depending on how many layers are being sent:
///
/// ## One Layer (current level only - no child structures):
/// - Send Phase 1, receive results
/// - **Skip Phase 2** - use Phase 1 results directly for review
/// - This applies at ANY level (parent, child, grandchild, etc.)
/// - When a level has no children to process, it becomes "one layer"
///
/// ## Multi-Layer (current level + child structures):
/// - **Current Level**: Send Phase 1, receive, send Phase 2, receive updated → use Phase 2 for review and child calls
/// - **Child Level**: Send Phase 1 with latest parent data, receive, send Phase 2, receive updated → use Phase 2 for review and grandchild calls
/// - **Deepest Level** (no more children below): Send Phase 1, receive → use directly (becomes "one layer")
///
/// Key insight: At each level, if there are child structures below (`ai_planned_structure_paths` not empty),
/// do two phases. If no children below (empty paths), skip Phase 2 and use Phase 1 directly.
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

            // OPTIMIZATION: Skip Phase 2 in two cases:
            // 1. No child structures to process (ai_planned_structure_paths is empty)
            // 2. AI returned no NEW rows (only originals) - nothing new to review at parent level
            let skip_phase2_one_layer = state.ai_planned_structure_paths.is_empty();
            let skip_phase2_no_new_rows = extra_slice.is_empty();

            if skip_phase2_one_layer || skip_phase2_no_new_rows {
                if skip_phase2_one_layer {
                    info!(
                        "ONE-LAYER OPTIMIZATION: Skipping Phase 2 (no child structures to process), using Phase 1 results directly"
                    );
                } else {
                    info!(
                        "NO-NEW-ROWS OPTIMIZATION: Skipping Phase 2 (AI returned no new rows), using Phase 1 data for structures"
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
