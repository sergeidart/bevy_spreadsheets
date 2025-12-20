use crate::sheets::systems::ai::processor::DirectorSession;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState};

/// Cancel the AI batch review, clearing all review state and resetting UI modes.
/// 
/// If a DirectorSession is provided, it will be properly cancelled to set the
/// correct ProcessingStatus::Cancelled state.
pub fn cancel_batch(state: &mut EditorWindowState, session: Option<&mut DirectorSession>) {
    // Cancel Director session if active
    if let Some(session) = session {
        crate::sheets::systems::ai::processor::cancel_director_session(session, state);
    }
    
    state.ai_batch_review_active = false;
    state.ai_mode = AiModeState::Idle;
    state.ai_row_reviews.clear();
    state.ai_new_row_reviews.clear();
    state.ai_selected_rows.clear();
    state.ai_structure_detail_context = None;
    // Clear batch processing context
    state.ai_batch_context = None;
    // Also reset broader interaction modes and selections so the UI returns to normal (hides "Exit AI").
    state.reset_interaction_modes_and_selections();
}
