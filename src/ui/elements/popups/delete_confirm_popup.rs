// src/ui/elements/popups/delete_confirm_popup.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestDeleteSheet;
use crate::ui::elements::editor::EditorWindowState;

pub fn show_delete_confirm_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    delete_event_writer: &mut EventWriter<RequestDeleteSheet>,
) {
    // Only proceed if the popup should be shown according to the state
    if !state.show_delete_confirm_popup {
        return;
    }

    let mut delete_confirm_popup_open = state.show_delete_confirm_popup; // Sync with current state
    let mut cancel_clicked = false; // Flag for cancel
    let mut delete_clicked = false; // Flag for delete action

    egui::Window::new("Confirm Delete")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut delete_confirm_popup_open) // Bind to the temporary variable
        .show(ctx, |ui| {
            ui.label(format!(
                "Permanently delete sheet '{:?}/{}'?", // Show category
                state.delete_target_category,
                state.delete_target_sheet
            ));
            ui.label("This will also delete the associated file(s) if they exist.");
            ui.colored_label(egui::Color32::YELLOW, "This action cannot be undone.");
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("DELETE").fill(egui::Color32::DARK_RED))
                    .clicked()
                {
                    delete_clicked = true; // Set flag
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true; // Set flag
                }
            });
        }); // End .show()

    // --- Logic AFTER the window UI ---
    let mut close_popup = false;

    if delete_clicked {
        delete_event_writer.write(RequestDeleteSheet {
            category: state.delete_target_category.clone(), // <<< Send category
            sheet_name: state.delete_target_sheet.clone(),
        });
        close_popup = true;
    }

    if cancel_clicked {
        close_popup = true;
    }
    if !close_popup && !delete_confirm_popup_open {
        close_popup = true;
    }

    if close_popup {
        state.show_delete_confirm_popup = false;
        state.delete_target_category = None; // Clear category
        state.delete_target_sheet.clear();
    } else {
        state.show_delete_confirm_popup = delete_confirm_popup_open;
    }
}