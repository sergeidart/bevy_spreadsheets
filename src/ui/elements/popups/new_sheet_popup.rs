// src/ui/elements/popups/new_sheet_popup.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::RequestCreateNewSheet;
use crate::ui::elements::editor::state::EditorWindowState;

pub fn show_new_sheet_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    create_sheet_writer: &mut EventWriter<RequestCreateNewSheet>,
    // ui_feedback: &UiFeedbackState, // To display error messages from name validation (optional here)
) {
    if !state.show_new_sheet_popup {
        return;
    }

    let mut popup_open = state.show_new_sheet_popup;
    let mut trigger_create = false;
    let mut cancel_clicked = false;

    // Determine target category string for display
    let target_category_display = state
        .new_sheet_target_category
        .as_deref()
        .unwrap_or("Root");

    egui::Window::new("Create New Sheet")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut popup_open)
        .show(ctx, |ui| {
            ui.label(format!(
                "Enter name for the new sheet in category: '{}'",
                target_category_display
            ));
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Sheet Name:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.new_sheet_name_input)
                        .desired_width(200.0)
                        .lock_focus(true),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !state.new_sheet_name_input.trim().is_empty() {
                        trigger_create = true;
                    }
                }
            });
            ui.small("Allowed characters: A-Z, a-z, 0-9, space, underscore, hyphen. Cannot be empty.");


            // Placeholder for potential error messages from previous validation if implemented directly here
            // if ui_feedback.is_error && ui_feedback.last_message.contains("Sheet name") {
            //     ui.colored_label(egui::Color32::RED, &ui_feedback.last_message);
            // }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_enabled(!state.new_sheet_name_input.trim().is_empty(), egui::Button::new("Create")).clicked() {
                    trigger_create = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true;
                }
            });
        });

    if trigger_create {
        let trimmed_name = state.new_sheet_name_input.trim();
        if !trimmed_name.is_empty() {
            // Basic client-side validation (more thorough validation in the handler system)
            if validator_basic_sheet_name(trimmed_name) {
                create_sheet_writer.write(RequestCreateNewSheet {
                    desired_name: trimmed_name.to_string(),
                    category: state.new_sheet_target_category.clone(),
                });
                state.show_new_sheet_popup = false; // Close on successful request send
                state.new_sheet_name_input.clear();
            } else {
                // Optionally, provide immediate feedback here or rely on system feedback
                warn!("New sheet name '{}' contains invalid characters.", trimmed_name);
                // Could set a local error string in `state` to display in the popup
            }
        }
    }

    if cancel_clicked || !popup_open {
        state.show_new_sheet_popup = false;
        state.new_sheet_name_input.clear();
        state.new_sheet_target_category = None; // Clear target category
    }
}

// Basic client-side validator (can be expanded)
fn validator_basic_sheet_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return false;
    }
    // Allow spaces, alphanumeric, underscore, hyphen
    name.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '_' || c == '-')
}