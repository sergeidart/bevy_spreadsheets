// src/ui/elements/editor/ai_helpers.rs
use bevy::prelude::*;
use super::state::{AiModeState, EditorWindowState, ReviewChoice};

// --- MODIFIED: Add pub(super) to make it visible to sibling modules ---
pub(super) fn setup_review_for_index(state: &mut EditorWindowState, queue_index: usize) -> bool {
// --- END MODIFIED ---
    if let Some(original_row_index) = state.ai_review_queue.get(queue_index).cloned() {
        if let Some(suggestion) = state.ai_suggestions.remove(&original_row_index) {
            let num_cols = suggestion.len();
            state.current_ai_suggestion_edit_buffer = Some((original_row_index, suggestion));
            state.ai_review_column_choices = vec![ReviewChoice::AI; num_cols];
            state.ai_current_review_index = Some(queue_index);
            info!("Setting up review for original row index: {}", original_row_index);
            return true;
        } else {
            warn!(
                "Suggestion for original row index {} (queue index {}) missing from map!",
                original_row_index, queue_index
            );
        }
    }
    false
}

pub(super) fn advance_review_queue(state: &mut EditorWindowState) {
    state.current_ai_suggestion_edit_buffer = None;
    state.ai_review_column_choices.clear();

    if let Some(current_queue_idx) = state.ai_current_review_index {
        let mut next_queue_idx = current_queue_idx + 1;
        while next_queue_idx < state.ai_review_queue.len() {
            if setup_review_for_index(state, next_queue_idx) {
                return;
            }
            next_queue_idx += 1;
        }
        info!("Review queue finished or no more valid suggestions found.");
        exit_review_mode(state);
    } else {
        warn!("advance_review_queue called with no current index.");
        exit_review_mode(state);
    }
}

pub(super) fn exit_review_mode(state: &mut EditorWindowState) {
     info!("Exiting AI review mode.");
    state.ai_mode = AiModeState::Idle;
    state.ai_suggestions.clear();
    state.ai_review_queue.clear();
    state.ai_current_review_index = None;
    state.current_ai_suggestion_edit_buffer = None;
    state.ai_review_column_choices.clear();
    state.ai_selected_rows.clear();
}