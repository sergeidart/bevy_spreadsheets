// src/ui/elements/editor/ai_helpers.rs
use bevy::prelude::*;
use super::state::{AiModeState, EditorWindowState};

/// Helper to advance the review queue or exit review mode.
pub(super) fn advance_review_queue(state: &mut EditorWindowState) {
    if let Some(queue_idx) = state.ai_current_review_index {
        if queue_idx + 1 < state.ai_review_queue.len() {
            // More items left, advance index
            state.ai_current_review_index = Some(queue_idx + 1);
        } else {
            // Reached end of queue
            info!("Review queue finished.");
            exit_review_mode(state);
        }
    } else {
        // Should not happen if called correctly, but exit defensively
        warn!("advance_review_queue called with no current index.");
        exit_review_mode(state);
    }
}

/// Helper to clean up state when exiting review mode.
pub(super) fn exit_review_mode(state: &mut EditorWindowState) {
     info!("Exiting AI review mode.");
    state.ai_mode = AiModeState::Idle;
    state.ai_suggestions.clear(); // Clear remaining suggestions on exit
    state.ai_review_queue.clear();
    state.ai_current_review_index = None;
    state.ai_selected_rows.clear(); // Clear original selection too
}