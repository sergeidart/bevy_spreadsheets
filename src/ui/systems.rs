// src/ui/systems.rs
use crate::{
    sheets::events::{AiTaskResult, SheetOperationFeedback},
    // --- MODIFIED: Import EditorWindowState as it's now a Resource ---
    ui::{
        elements::editor::state::{AiModeState, EditorWindowState}, // Keep AiModeState
        UiFeedbackState,
    },
    // --- END MODIFIED ---
};
use bevy::prelude::*;
use std::any;

pub fn handle_ui_feedback(
    mut feedback_events: EventReader<SheetOperationFeedback>,
    mut ui_feedback_state: ResMut<UiFeedbackState>,
) {
    let mut last_message = None;
    for event in feedback_events.read() {
        last_message = Some((event.message.clone(), event.is_error));
        if !event.is_error {
             break;
        }
    }
    if let Some((msg, is_error)) = last_message {
         ui_feedback_state.last_message = msg;
         ui_feedback_state.is_error = is_error;
         if is_error {
             warn!("UI Feedback (Error): {}", ui_feedback_state.last_message);
         } else {
             info!("UI Feedback: {}", ui_feedback_state.last_message);
         }
    }
}

// --- MODIFIED: Change Local to ResMut ---
pub fn handle_ai_task_results(
    mut ev_ai_results: EventReader<AiTaskResult>,
    mut state: ResMut<EditorWindowState>, // Changed from Local
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
// --- END MODIFIED ---
    debug!("handle_ai_task_results checking for events. Current AI Mode: {:?}", state.ai_mode);
    if state.ai_mode != AiModeState::Submitting && state.ai_mode != AiModeState::ResultsReady {
        if !ev_ai_results.is_empty() {
            let event_count = ev_ai_results.len();
            info!(
                "Ignoring {} AI result(s) received while not in Submitting/ResultsReady state (current: {:?})",
                event_count, state.ai_mode
            );
            ev_ai_results.clear();
        }
        return;
    }

    let mut received_at_least_one_result = false;
    let mut all_tasks_successful = true;

    if !ev_ai_results.is_empty() {
        debug!("Processing {} AiTaskResult events.", ev_ai_results.len());
    }

    for ev in ev_ai_results.read() {
        received_at_least_one_result = true;
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
                error!("  AI Task Failure for row {}: {}", ev.original_row_index, err_msg);
                feedback_writer.send(SheetOperationFeedback {
                    message: format!("AI Error (Row {}): {}", ev.original_row_index, err_msg),
                    is_error: true,
                });
                if all_tasks_successful {
                    state.ai_prompt_display = format!(
                        "AI Processing Error for row {}:\n{}\n\n{}",
                        ev.original_row_index,
                        err_msg,
                        state.ai_prompt_display.lines().skip_while(|l| l.starts_with("AI Processing Error")).collect::<Vec<_>>().join("\n")
                    );
                }
                all_tasks_successful = false;
            }
        }
    }

    if received_at_least_one_result && state.ai_mode == AiModeState::Submitting {
        if all_tasks_successful {
            info!("All AI results received successfully, moving to ResultsReady state.");
            state.ai_mode = AiModeState::ResultsReady;
        } else {
            error!("One or more AI tasks failed. Reverting to Preparing state.");
            state.ai_mode = AiModeState::Preparing;
            state.ai_suggestions.clear();
        }
    }
}


#[derive(Component)]
pub struct SendEvent<E: Event> {
    pub event: E,
}

pub fn forward_events<E: Event + Clone + std::fmt::Debug>(
    mut commands: Commands,
    mut writer: EventWriter<E>,
    query: Query<(Entity, &SendEvent<E>)>,
    mut event_type_name: Local<String>,
) {
    if event_type_name.is_empty() {
        *event_type_name = any::type_name::<E>().split("::").last().unwrap_or("UnknownEvent").to_string();
    }

    let mut count = 0;
    for (entity, send_event_component) in query.iter() {
        count += 1;
        debug!("Forwarding event type '{}' #{}: {:?}", *event_type_name, count, send_event_component.event);
        writer.send(send_event_component.event.clone());
        commands.entity(entity).remove::<SendEvent<E>>();
        commands.entity(entity).despawn_recursive();
    }

    if count > 0 {
        info!("Forwarded {} instance(s) of event type '{}'.", count, *event_type_name);
    }
}