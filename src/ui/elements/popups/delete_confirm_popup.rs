// src/ui/elements/popups/delete_confirm_popup.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestDeleteSheet;
use crate::ui::elements::editor::EditorWindowState; // Use state defined in editor

/// Displays the "Confirm Delete" popup window if state.show_delete_confirm_popup is true.
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
                ui.label("This will also delete the associated file(s) if they exist.");
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