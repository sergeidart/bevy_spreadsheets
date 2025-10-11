use crate::ui::elements::editor::state::{AiModeState, EditorWindowState};

/// Cancel the AI batch review, clearing all review state and resetting UI modes.
pub fn cancel_batch(state: &mut EditorWindowState) {
    state.ai_batch_review_active = false;
    state.ai_mode = AiModeState::Idle;
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    state.ai_selected_rows.clear();
    state.ai_structure_detail_context = None;
    // Clear Phase 1/2 state
    state.ai_phase1_intermediate = None;
    state.ai_expecting_phase2_result = false;
    // Also reset broader interaction modes and selections so the UI returns to normal (hides "Exit AI").
    state.reset_interaction_modes_and_selections();
}
