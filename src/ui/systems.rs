// src/ui/systems.rs
use bevy::prelude::*;
use crate::sheets::events::SheetOperationFeedback;
use super::UiFeedbackState; // Assuming you created resources.rs

// Or if UiFeedbackState is in ui/mod.rs:
// use super::UiFeedbackState;


/// System to read feedback events and update the UI feedback resource.
pub fn handle_ui_feedback(
    mut feedback_events: EventReader<SheetOperationFeedback>,
    mut ui_feedback_state: ResMut<UiFeedbackState>,
) {
    for event in feedback_events.read() {
        // Update the resource with the latest message
        ui_feedback_state.last_message = event.message.clone();
        ui_feedback_state.is_error = event.is_error;
        // Optionally log feedback events too
        if event.is_error {
            warn!("UI Feedback (Error): {}", event.message);
        } else {
            info!("UI Feedback: {}", event.message);
        }
    }
}