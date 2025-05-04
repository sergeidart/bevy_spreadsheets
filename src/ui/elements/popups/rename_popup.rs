// src/ui/elements/popups/rename_popup.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestRenameSheet;
use crate::ui::elements::editor::EditorWindowState; // Use state defined in editor
use crate::ui::UiFeedbackState;

/// Displays the "Rename Sheet" popup window if state.show_rename_popup is true.
pub fn show_rename_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState, // Needs mutable state access
    rename_event_writer: &mut EventWriter<RequestRenameSheet>,
    ui_feedback: &UiFeedbackState, // Read-only feedback
) {
    // Use a temporary variable to control window visibility via egui::Window::open
    let mut rename_popup_open = state.show_rename_popup;
    // Flag to defer event sending until after UI scope
    let mut trigger_rename = false;

    if state.show_rename_popup {
        egui::Window::new("Rename Sheet")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut rename_popup_open) // Bind window open state
            .show(ctx, |ui| {
                ui.label(format!("Renaming sheet: '{}'", state.rename_target));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("New Name:");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut state.new_name_input)
                            .desired_width(150.0)
                            .lock_focus(true), // Auto-focus on open
                    );
                    // Check for Enter key after interaction
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                         if !state.new_name_input.trim().is_empty() { // Add check
                            trigger_rename = true;
                         } else {
                             // Optional: feedback for empty name on Enter
                         }
                    }
                });

                // Display validation errors from the feedback system
                if ui_feedback.is_error && ui_feedback.last_message.contains("Rename failed") {
                    ui.colored_label(egui::Color32::RED, &ui_feedback.last_message);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.add_enabled(!state.new_name_input.trim().is_empty(), egui::Button::new("Rename")).clicked() {
                        trigger_rename = true; // Set flag
                    }
                    if ui.button("Cancel").clicked() {
                        // Close immediately on cancel
                        state.show_rename_popup = false;
                    }
                });
            });

        // Send event if flagged (outside the immediate UI scope)
        if trigger_rename {
             // Basic validation already done for button enable state, but double check
             if !state.new_name_input.trim().is_empty() {
                 rename_event_writer.send(RequestRenameSheet {
                     old_name: state.rename_target.clone(),
                     new_name: state.new_name_input.clone(),
                 });
                 // Assume success for now, let feedback show errors
                 state.show_rename_popup = false;
             }
        }

        // Update state based on window interaction (closing via 'x')
        // If user closed via 'x', rename_popup_open will be false here
        state.show_rename_popup = rename_popup_open;

        // Reset internal state if the popup is no longer shown
        if !state.show_rename_popup {
            state.rename_target.clear();
            state.new_name_input.clear();
        }
    }
}