// src/ui/elements/popups/rename_popup.rs

use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::events::{RequestRenameCategory, RequestRenameSheet};
use crate::ui::elements::editor::EditorWindowState;
use crate::ui::UiFeedbackState;

pub fn show_rename_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    rename_sheet_writer: &mut EventWriter<RequestRenameSheet>,
    rename_category_writer: &mut EventWriter<RequestRenameCategory>,
    ui_feedback: &UiFeedbackState,
) {
    // Only proceed if the popup should be shown according to the state
    if !state.show_rename_popup {
        return;
    }

    let mut rename_popup_open = state.show_rename_popup; // Sync with current state
    let mut cancel_clicked = false; // Flag for cancel
    let mut trigger_rename = false; // Flag for rename action

    // Determine if we're renaming a sheet or a category
    let renaming_category =
        state.rename_target_sheet.is_empty() && state.rename_target_category.is_some();
    // Keep the generic head title as "Rename" per request
    let window_title = "Rename";

    egui::Window::new(window_title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut rename_popup_open) // Bind to the temporary variable
        .show(ctx, |ui| {
            // Simplified UI: only show the input row (no extra descriptive line)
            ui.horizontal(|ui| {
                ui.label("New Name:");
                // Prefill input if empty when opening
                let mut just_opened_prefill = false;
                if state.new_name_input.is_empty() {
                    if renaming_category {
                        state.new_name_input =
                            state.rename_target_category.clone().unwrap_or_default();
                    } else {
                        state.new_name_input = state.rename_target_sheet.clone();
                    }
                    // We just populated the input, so treat as first frame of opening
                    just_opened_prefill = true;
                }
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.new_name_input)
                        .desired_width(150.0)
                        .lock_focus(true),
                );
                // Autofocus the input when the popup first opens
                if just_opened_prefill {
                    response.request_focus();
                }
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !state.new_name_input.trim().is_empty() {
                        trigger_rename = true;
                    }
                }
            });

            if ui_feedback.is_error && ui_feedback.last_message.contains("Rename failed") {
                ui.colored_label(egui::Color32::RED, &ui_feedback.last_message);
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.new_name_input.trim().is_empty(),
                        egui::Button::new("Rename"),
                    )
                    .clicked()
                {
                    trigger_rename = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true;
                }
            });
        });

    // --- Logic AFTER the window UI ---
    let mut close_popup = false;

    if trigger_rename {
        if !state.new_name_input.trim().is_empty() {
            if renaming_category {
                if let Some(old_cat) = state.rename_target_category.clone() {
                    let new_cat = state.new_name_input.clone();
                    rename_category_writer.write(RequestRenameCategory {
                        old_name: old_cat.clone(),
                        new_name: new_cat.clone(),
                    });
                    // Optimistically update selection if we're renaming the currently selected category
                    if state.selected_category.as_deref() == Some(old_cat.as_str()) {
                        state.selected_category = Some(new_cat);
                    }
                }
            } else {
                rename_sheet_writer.write(RequestRenameSheet {
                    category: state.rename_target_category.clone(),
                    old_name: state.rename_target_sheet.clone(),
                    new_name: state.new_name_input.clone(),
                });
            }
            close_popup = true;
        }
    }

    if cancel_clicked {
        close_popup = true;
    }
    if !close_popup && !rename_popup_open {
        close_popup = true;
    }

    if close_popup {
        state.show_rename_popup = false;
        state.rename_target_category = None; // Clear category
        state.rename_target_sheet.clear();
        state.new_name_input.clear();
    } else {
        state.show_rename_popup = rename_popup_open;
    }
}
