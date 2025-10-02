// src/sheets/systems/ai/legacy.rs
// Legacy single-row AI task result handler
// NOTE: This may not be used anymore - kept for backwards compatibility
// Consider removing if confirmed unused

use bevy::prelude::*;

use crate::sheets::events::{AiTaskResult, SheetOperationFeedback};
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState, ReviewChoice, RowReview};

/// Handle single-row AI task results (non-batch, legacy system)
/// NOTE: This handler may be deprecated - verify usage before removal
pub fn handle_ai_task_results(
    mut ev_ai_results: EventReader<AiTaskResult>,
    mut state: ResMut<EditorWindowState>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    if ev_ai_results.is_empty() {
        return;
    }
    debug!(
        "handle_ai_task_results: processing {} event(s). Current AI Mode: {:?}",
        ev_ai_results.len(),
        state.ai_mode
    );
    if state.ai_mode != AiModeState::Submitting && state.ai_mode != AiModeState::ResultsReady {
        let event_count = ev_ai_results.len();
        info!(
            "Ignoring {} AI result(s) received while not in Submitting/ResultsReady state (current: {:?})",
            event_count, state.ai_mode
        );
        ev_ai_results.clear(); // Consume events
        return;
    }

    let mut received_at_least_one_result = false;
    let mut all_tasks_successful_this_batch = true;

    for ev in ev_ai_results.read() {
        received_at_least_one_result = true;
        info!(
            "Received AI task result for row {}. Raw response present: {}",
            ev.original_row_index,
            ev.raw_response.is_some()
        );

        if let Some(raw) = &ev.raw_response {
            state.ai_raw_output_display = raw.clone();
        } else if let Err(e) = &ev.result {
            state.ai_raw_output_display = format!(
                "Error processing AI result for row {}: {}",
                ev.original_row_index, e
            );
        }

        match &ev.result {
            Ok(suggestion) => {
                // Re-expand suggestion back to full column width using mapping stored in event
                let expanded = if !ev.included_non_structure_columns.is_empty() {
                    let max_col = *ev.included_non_structure_columns.iter().max().unwrap_or(&0);
                    let mut row_buf = vec![String::new(); max_col + 1];
                    for (i, actual_col) in ev.included_non_structure_columns.iter().enumerate() {
                        let src_index = i + ev.context_only_prefix_count; // skip context-only prefix
                        if let Some(val) = suggestion.get(src_index) {
                            if let Some(slot) = row_buf.get_mut(*actual_col) {
                                *slot = val.clone();
                            }
                        }
                    }
                    row_buf
                } else {
                    suggestion.clone()
                };
                let included = ev.included_non_structure_columns.clone();
                let mut original_snapshot: Vec<String> = Vec::with_capacity(included.len());
                let mut ai_snapshot: Vec<String> = Vec::with_capacity(included.len());
                for (logical_i, _actual_col) in included.iter().enumerate() {
                    original_snapshot.push(String::new());
                    ai_snapshot.push(expanded.get(logical_i).cloned().unwrap_or_default());
                }
                state.ai_row_reviews.push(RowReview {
                    row_index: ev.original_row_index,
                    original: original_snapshot,
                    ai: ai_snapshot,
                    choices: vec![ReviewChoice::AI; included.len()],
                    non_structure_columns: included,
                });
                state.ai_batch_review_active = true;
            }
            Err(err_msg) => {
                feedback_writer.write(SheetOperationFeedback {
                    message: format!("AI Error (Row {}): {}", ev.original_row_index, err_msg),
                    is_error: true,
                });
                if let Some(raw) = &ev.raw_response {
                    state.ai_raw_output_display = format!(
                        "Row {} Error: {}\n--- Raw Model Output ---\n{}",
                        ev.original_row_index, err_msg, raw
                    );
                }
                state.ai_output_panel_visible = true;
                all_tasks_successful_this_batch = false;
            }
        }
    }

    if received_at_least_one_result && state.ai_mode == AiModeState::Submitting {
        if all_tasks_successful_this_batch {
            state.ai_mode = AiModeState::ResultsReady;
        } else {
            state.ai_mode = AiModeState::Preparing;
            state.ai_row_reviews.clear();
        }
    }
}
