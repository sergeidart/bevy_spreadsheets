// src/ui/systems.rs
use crate::{
    sheets::events::{AiTaskResult, SheetOperationFeedback},
    ui::{
        elements::editor::state::{AiModeState, EditorWindowState},
        UiFeedbackState,
    },
    sheets::resources::SheetRegistry,
    sheets::systems::io::save::save_single_sheet,
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
                // Re-expand suggestion back to full column width using mapping
                let expanded = if !state.ai_included_non_structure_columns.is_empty() {
                    // Determine required length (max index +1)
                    let max_col = *state.ai_included_non_structure_columns.iter().max().unwrap_or(&0);
                    let mut row_buf = vec![String::new(); max_col + 1];
                    for (i, actual_col) in state.ai_included_non_structure_columns.iter().enumerate() {
                        let src_index = i + state.ai_context_only_prefix_count; // skip context-only prefix
                        if let Some(val) = suggestion.get(src_index) { if let Some(slot) = row_buf.get_mut(*actual_col) { *slot = val.clone(); } }
                    }
                    row_buf
                } else { suggestion.clone() };
                state.ai_suggestions.insert(ev.original_row_index, expanded);
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

// Apply pending structure key column selection (if any) and compute ancestor chain
pub fn apply_pending_structure_key_selection(
    mut state: ResMut<EditorWindowState>,
    mut registry: ResMut<SheetRegistry>,
) {
    if let Some((cat, sheet, structure_col_index, new_key_opt)) = state.pending_structure_key_apply.take() {
        let mut root_parent_link: Option<crate::sheets::definitions::StructureParentLink> = None;
        let mut changed = false;
        if let Some(sheet_data) = registry.get_sheet_mut(&cat, &sheet) {
            if let Some(meta) = &mut sheet_data.metadata {
                if let Some(col) = meta.columns.get_mut(structure_col_index) {
                    if col.structure_key_parent_column_index != new_key_opt { changed = true; }
                    col.structure_key_parent_column_index = new_key_opt;
                    col.structure_ancestor_key_parent_column_indices = Some(Vec::new());
                }
                root_parent_link = meta.structure_parent.clone();
            }
        }
        let mut collected: Vec<usize> = Vec::new();
        let mut current_parent = root_parent_link;
        let mut safety = 0;
        while let Some(parent_link) = current_parent.clone() {
            if safety > 32 { break; }
            safety += 1;
            if let Some(parent_sheet) = registry.get_sheet(&parent_link.parent_category, &parent_link.parent_sheet) {
                if let Some(parent_meta) = &parent_sheet.metadata {
                    if let Some(parent_col) = parent_meta.columns.get(parent_link.parent_column_index) {
                        if let Some(kidx) = parent_col.structure_key_parent_column_index { collected.push(kidx); }
                    }
                    current_parent = parent_meta.structure_parent.clone();
                    continue;
                }
            }
            break;
        }
        collected.reverse();
        if let Some(sheet_data) = registry.get_sheet_mut(&cat, &sheet) {
            if let Some(meta) = &mut sheet_data.metadata {
                if let Some(col) = meta.columns.get_mut(structure_col_index) {
                    let existing = col.structure_ancestor_key_parent_column_indices.clone().unwrap_or_default();
                    if existing != collected { changed = true; }
                    col.structure_ancestor_key_parent_column_indices = Some(collected);
                }
                let meta_clone_for_save = if changed { Some(meta.clone()) } else { None };
                if let Some(meta_clone) = meta_clone_for_save { save_single_sheet(registry.as_ref(), &meta_clone); info!("Persisted structure key selection for column {} in '{:?}/{}'", structure_col_index + 1, cat, sheet); }
            }
        }
    }
}