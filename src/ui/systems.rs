// src/ui/systems.rs
use crate::{
    sheets::events::{AiTaskResult, SheetOperationFeedback},
    ui::{
        elements::editor::state::{AiModeState, EditorWindowState},
        UiFeedbackState,
    },
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
        // Prioritize showing the first non-error, or the last error
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

pub fn handle_ai_task_results(
    mut ev_ai_results: EventReader<AiTaskResult>,
    mut state: ResMut<EditorWindowState>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    debug!("handle_ai_task_results checking for events. Current AI Mode: {:?}", state.ai_mode);
    if state.ai_mode != AiModeState::Submitting && state.ai_mode != AiModeState::ResultsReady {
        if !ev_ai_results.is_empty() {
            let event_count = ev_ai_results.len();
            info!(
                "Ignoring {} AI result(s) received while not in Submitting/ResultsReady state (current: {:?})",
                event_count, state.ai_mode
            );
            ev_ai_results.clear(); // Consume events
        }
        return;
    }

    let mut received_at_least_one_result = false;
    let mut all_tasks_successful_this_batch = true; // Track success for this batch of events

    for ev in ev_ai_results.read() {
        received_at_least_one_result = true;
        info!(
            "Received AI task result for row {}. Raw response present: {}",
            ev.original_row_index,
            ev.raw_response.is_some()
        );

        // Update the raw output display first
        if let Some(raw) = &ev.raw_response {
            state.ai_raw_output_display = raw.clone();
        } else if let Err(e) = &ev.result {
            // If no raw response but there's an error, display the error
            state.ai_raw_output_display = format!("Error processing AI result for row {}: {}", ev.original_row_index, e);
        }


        match &ev.result {
            Ok(suggestion) => {
                info!("  AI Task Success for row {}: {:?}", ev.original_row_index, suggestion);
                state
                    .ai_suggestions
                    .insert(ev.original_row_index, suggestion.clone());
                // Don't clear ai_raw_output_display here, let it show the successful raw response
            }
            Err(err_msg) => {
                error!("  AI Task Failure for row {}: {}", ev.original_row_index, err_msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: format!("AI Error (Row {}): {}", ev.original_row_index, err_msg),
                    is_error: true,
                });
                // The raw_output_display is already set with the error or raw response
                all_tasks_successful_this_batch = false;
            }
        }
    }

    if received_at_least_one_result && state.ai_mode == AiModeState::Submitting {
        if all_tasks_successful_this_batch {
            info!("All AI results in this batch processed successfully, moving to ResultsReady state.");
            state.ai_mode = AiModeState::ResultsReady;
            // state.ai_prompt_display can remain to show what was sent
        } else {
            error!("One or more AI tasks failed in this batch. Reverting to Preparing state.");
            state.ai_mode = AiModeState::Preparing; // Or Idle, depending on desired flow
            state.ai_suggestions.clear(); // Clear any partial suggestions
            // state.ai_raw_output_display will show the last error/raw output
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
        writer.write(send_event_component.event.clone());
        commands.entity(entity).remove::<SendEvent<E>>();
        commands.entity(entity).despawn(); // Despawn entity after forwarding
    }

    if count > 0 {
        info!("Forwarded {} instance(s) of event type '{}'.", count, *event_type_name);
    }
}

/// Clears transient UI feedback when a sheet's data is modified in the registry.
/// This ensures the "service row" (status/error message) is hidden after switching or
/// changing sheets and will reappear if a new feedback event is emitted afterwards.
pub fn clear_ui_feedback_on_sheet_change(
    state: Res<EditorWindowState>,
    mut ui_feedback_state: ResMut<UiFeedbackState>,
    mut last_selection: Local<Option<(Option<String>, Option<String>)>>,
) {
    // Determine current selection tuple
    let current_sel = (state.selected_category.clone(), state.selected_sheet_name.clone());

    // If we have a last selection and it's different -> user switched sheets (open another one)
    if let Some(prev) = last_selection.as_ref() {
        if prev != &current_sel {
            // Only clear when the selection actually changed (not on initial startup)
            ui_feedback_state.last_message.clear();
            ui_feedback_state.is_error = false;
            trace!("Cleared UI feedback due to sheet selection change.");
        }
    }

    // Update last selection stored locally
    *last_selection = Some(current_sel);
}