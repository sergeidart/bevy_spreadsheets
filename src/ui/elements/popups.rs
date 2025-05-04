// src/ui/elements/popups.rs
use bevy::prelude::*;
use bevy_egui::egui;

// Use events directly qualified to avoid import conflicts if necessary
use crate::sheets::events::{RequestRenameSheet, RequestDeleteSheet, RequestUpdateColumnName}; // Added Event
// Use state definitions potentially shared across UI elements
use super::editor::EditorWindowState; // Use state defined in editor for now
use crate::ui::UiFeedbackState; // Use feedback state resource

/// Displays the "Rename Sheet" popup window if state.show_rename_popup is true.
/// Handles user input and sends the RequestRenameSheet event.
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
                        trigger_rename = true;
                    }
                });

                // Display validation errors from the feedback system
                if ui_feedback.is_error && ui_feedback.last_message.contains("Rename failed") {
                    ui.colored_label(egui::Color32::RED, &ui_feedback.last_message);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
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
            rename_event_writer.send(RequestRenameSheet {
                old_name: state.rename_target.clone(),
                new_name: state.new_name_input.clone(),
            });
            // Assume success for now, let feedback show errors
            state.show_rename_popup = false;
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

/// Displays the "Confirm Delete" popup window if state.show_delete_confirm_popup is true.
/// Handles user confirmation and sends the RequestDeleteSheet event.
pub fn show_delete_confirm_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState, // Needs mutable state access
    delete_event_writer: &mut EventWriter<RequestDeleteSheet>,
) {
    // Use a temporary variable to control window visibility via egui::Window::open
    let mut delete_confirm_popup_open = state.show_delete_confirm_popup;

    if state.show_delete_confirm_popup {
        egui::Window::new("Confirm Delete")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut delete_confirm_popup_open) // Bind window open state
            .show(ctx, |ui| {
                ui.label(format!(
                    "Permanently delete sheet '{}'?",
                    state.delete_target
                ));
                ui.label("This will also delete the associated file if it exists.");
                ui.colored_label(egui::Color32::YELLOW, "This action cannot be undone.");
                ui.separator();
                ui.horizontal(|ui| {
                    // Add stronger visual cue for destructive action
                    if ui
                        .add(egui::Button::new("DELETE").fill(egui::Color32::DARK_RED))
                        .clicked()
                    {
                        // Send delete event
                        delete_event_writer.send(RequestDeleteSheet {
                            sheet_name: state.delete_target.clone(),
                        });
                        // Assume success for now; UI will update if sheet disappears
                        state.show_delete_confirm_popup = false;
                    }
                    if ui.button("Cancel").clicked() {
                        state.show_delete_confirm_popup = false; // Close immediately
                    }
                });
            });

        // Update state based on window interaction (closing via 'x')
        state.show_delete_confirm_popup = delete_confirm_popup_open;

        // Reset internal state if the popup is no longer shown
        if !state.show_delete_confirm_popup {
            state.delete_target.clear();
        }
    }
}

/// Displays the "Rename Column" popup window if state.show_column_rename_popup is true.
/// Handles user input and sends the RequestUpdateColumnName event.
pub fn show_column_rename_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    column_rename_writer: &mut EventWriter<RequestUpdateColumnName>,
    // Potentially add ui_feedback: &UiFeedbackState if you want validation feedback
) {
    let mut popup_open = state.show_column_rename_popup;
    let mut trigger_rename = false;

    if state.show_column_rename_popup {
        egui::Window::new("Rename Column")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut popup_open)
            .show(ctx, |ui| {
                ui.label(format!("Renaming column #{} for sheet: '{}'", state.rename_column_target_index + 1, state.rename_column_target_sheet));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("New Name:");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut state.rename_column_input)
                            .desired_width(150.0)
                            .lock_focus(true),
                    );
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        trigger_rename = true;
                    }
                });

                // TODO: Add validation feedback display if needed using ui_feedback Res
                // Example:
                // if ui_feedback.is_error && ui_feedback.last_message.contains("column for sheet") {
                //     ui.colored_label(egui::Color32::RED, &ui_feedback.last_message);
                // }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        trigger_rename = true;
                    }
                    if ui.button("Cancel").clicked() {
                        state.show_column_rename_popup = false; // Close immediately
                    }
                });
            });

        if trigger_rename && !state.rename_column_input.trim().is_empty() {
            column_rename_writer.send(RequestUpdateColumnName {
                sheet_name: state.rename_column_target_sheet.clone(),
                column_index: state.rename_column_target_index,
                new_name: state.rename_column_input.clone(),
            });
            state.show_column_rename_popup = false; // Close on success/attempt
        } else if trigger_rename {
             // Optional: Show feedback if name is empty
             warn!("Column rename cancelled: New name cannot be empty.");
             // Consider adding this feedback to the UiFeedbackState
             // feedback_writer.send( SheetOperationFeedback { message: "New column name cannot be empty".to_string(), is_error: true });
             // Need EventWriter<SheetOperationFeedback> for the above
        }


        // Update state based on window interaction (closing via 'x')
        state.show_column_rename_popup = popup_open;

        // Reset internal state if the popup is no longer shown
        if !state.show_column_rename_popup {
            state.rename_column_target_sheet.clear();
            state.rename_column_target_index = 0; // Reset index
            state.rename_column_input.clear();
        }
    }
}