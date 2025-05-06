// src/ui/systems.rs
use crate::{
    sheets::events::{AiTaskResult, SheetOperationFeedback}, // Import AiTaskResult from sheets
    ui::{
        elements::editor::state::{AiModeState, EditorWindowState}, // Import state
        UiFeedbackState,
    },
};
use bevy::prelude::*;

/// System to read feedback events and update the UI feedback resource.
pub fn handle_ui_feedback(
    mut feedback_events: EventReader<SheetOperationFeedback>,
    mut ui_feedback_state: ResMut<UiFeedbackState>,
) {
    // Give priority to non-error messages if multiple events arrive
    let mut last_message = None;
    for event in feedback_events.read() {
        last_message = Some((event.message.clone(), event.is_error));
        if !event.is_error { // Stop if we find a non-error message
             break;
        }
    }
    if let Some((msg, is_error)) = last_message {
         ui_feedback_state.last_message = msg;
         ui_feedback_state.is_error = is_error;
         // Optionally log feedback events too
         if is_error {
             warn!("UI Feedback (Error): {}", ui_feedback_state.last_message);
         } else {
             info!("UI Feedback: {}", ui_feedback_state.last_message);
         }
    }
}

/// System to handle results coming back from background AI tasks.
pub fn handle_ai_task_results(
    mut ev_ai_results: EventReader<AiTaskResult>,
    mut state: Local<EditorWindowState>, // Access UI state directly
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    if state.ai_mode != AiModeState::Submitting {
        if !ev_ai_results.is_empty() {
            info!(
                "Ignoring AI results received while not in Submitting state (current: {:?})",
                state.ai_mode
            );
            ev_ai_results.clear();
        }
        return;
    }

    let mut received_results = false;
    for ev in ev_ai_results.read() {
        received_results = true;
        info!(
            "Received AI task result for row {}",
            ev.original_row_index
        );
        match &ev.result {
            Ok(suggestion) => {
                info!("  Success: {:?}", suggestion);
                state
                    .ai_suggestions
                    .insert(ev.original_row_index, suggestion.clone());
            }
            Err(err_msg) => {
                error!("  Failure: {}", err_msg);
                feedback_writer.send(SheetOperationFeedback {
                    message: format!("AI Error: {}", err_msg),
                    is_error: true,
                });
                state.ai_mode = AiModeState::Preparing; // Revert to allow retry/cancel
                state.ai_suggestions.clear();
                return; // Stop processing on first error
            }
        }
    }

    if received_results && state.ai_mode == AiModeState::Submitting {
        info!("AI results received, moving to ResultsReady state.");
        state.ai_mode = AiModeState::ResultsReady;
    }
}


// --- ADDED: Helper component and system to send events from main thread callback ---
#[derive(Component)]
pub struct SendEvent<E: Event> { // Made pub if needed elsewhere, otherwise keep private
    pub event: E,
}

pub fn forward_events<E: Event + Clone>(
    mut commands: Commands,
    mut writer: EventWriter<E>,
    query: Query<(Entity, &SendEvent<E>)>, // Query for entities with the component
) {
    for (entity, send_event) in query.iter() {
        writer.send(send_event.event.clone()); // Send the event
        commands.entity(entity).remove::<SendEvent<E>>(); // Clean up component
        // Optionally despawn the temporary entity (might be useful)
        commands.entity(entity).despawn();
    }
}