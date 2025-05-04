// src/ui/elements/popups/rename_popup.rs

use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestRenameSheet;
use crate::ui::elements::editor::EditorWindowState;
use crate::ui::UiFeedbackState;

pub fn show_rename_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    rename_event_writer: &mut EventWriter<RequestRenameSheet>,
    ui_feedback: &UiFeedbackState,
) {
    // Only proceed if the popup should be shown according to the state
    if !state.show_rename_popup {
        return;
    }

    let mut rename_popup_open = state.show_rename_popup; // Sync with current state
    let mut cancel_clicked = false; // Flag for cancel
    let mut trigger_rename = false; // Flag for rename action

    egui::Window::new("Rename Sheet")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut rename_popup_open) // Bind to the temporary variable
        .show(ctx, |ui| {
            ui.label(format!("Renaming sheet: '{}'", state.rename_target));
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("New Name:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.new_name_input)
                        .desired_width(150.0)
                        .lock_focus(true),
                );
                // Set flag on Enter key press after interaction
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !state.new_name_input.trim().is_empty() {
                        trigger_rename = true; // Set flag
                    }
                }
            });

            if ui_feedback.is_error && ui_feedback.last_message.contains("Rename failed") {
                ui.colored_label(egui::Color32::RED, &ui_feedback.last_message);
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_enabled(!state.new_name_input.trim().is_empty(), egui::Button::new("Rename")).clicked() {
                    trigger_rename = true; // Set flag
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true; // Set flag
                }
            });
        }); // End .show()

    // --- Logic AFTER the window UI ---

    let mut close_popup = false;

    // 1. Handle rename action if triggered
    if trigger_rename {
        if !state.new_name_input.trim().is_empty() {
            rename_event_writer.send(RequestRenameSheet {
                old_name: state.rename_target.clone(),
                new_name: state.new_name_input.clone(),
            });
            close_popup = true; // Mark popup for closing on success
        }
        // If validation fails (e.g., empty name), close_popup remains false
    }

    // 2. Handle cancel action if clicked
    if cancel_clicked {
        close_popup = true;
    }

    // 3. Handle closing via 'x' button (if rename/cancel didn't already mark for closing)
    if !close_popup && !rename_popup_open {
        close_popup = true; // Window was closed via 'x'
    }

    // 4. Update the actual state variable if closing is needed
    if close_popup {
        state.show_rename_popup = false;
        // Reset internal state only when popup actually closes
        state.rename_target.clear();
        state.new_name_input.clear();
    } else {
         // If not closing, ensure state reflects the temporary variable
         // (in case it was opened/closed rapidly without button interaction)
         // This line might be redundant if the initial check `if !state.show_rename_popup` works,
         // but can be kept for robustness.
         state.show_rename_popup = rename_popup_open;
    }
}