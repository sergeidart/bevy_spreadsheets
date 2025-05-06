// src/ui/elements/popups/ai_rule_popup.rs
use crate::{
    sheets::{resources::SheetRegistry, systems::io::save::save_single_sheet},
    ui::elements::editor::EditorWindowState,
};
use bevy::prelude::*;
use bevy_egui::egui;

/// Displays the modal popup window for editing the AI General Rule.
pub fn show_ai_rule_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry, // Need mutable for saving
) {
    if !state.show_ai_rule_popup {
        return;
    }

    // Local copy to control window visibility via egui's .open()
    let mut is_window_open = state.show_ai_rule_popup;
    // Flags to signal actions taken inside the window closure
    let mut save_requested = false;
    let mut cancel_requested = false;

    egui::Window::new("Edit AI General Rule")
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut is_window_open) // Pass mutable local copy here
        .show(ctx, |ui| {
            // Display current sheet context
            ui.label(format!(
                "Editing Rule for Sheet: '{:?}/{}'",
                state.selected_category,
                state.selected_sheet_name.as_deref().unwrap_or("?")
            ));
            ui.separator();

            ui.label("Provide a general instruction or context for the AI processing this sheet:");

            // TextEdit for the rule
            ui.add_sized(
                [ui.available_width(), ui.available_height() - 50.0],
                egui::TextEdit::multiline(&mut state.ai_general_rule_input)
                    .hint_text("e.g., 'Correct spelling and grammar...'")
                    .desired_rows(5),
            );

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Save Rule").clicked() {
                    save_requested = true; // Signal intent to save and close
                }
                if ui.button("Cancel").clicked() {
                    cancel_requested = true; // Signal intent to cancel and close
                }
            });
        }); // Window borrow on is_window_open ends here

    // --- Process actions AFTER the window UI ---

    let mut close_popup = false;

    if save_requested {
        // Perform the save logic
        let category_clone = state.selected_category.clone();
        let sheet_name_clone = state.selected_sheet_name.clone();

        if let Some(sheet_name) = sheet_name_clone {
            let mut meta_to_save: Option<crate::sheets::definitions::SheetMetadata> = None;
            let mut changed = false;
            { // Mutable borrow scope for registry
                if let Some(sheet_mut) = registry.get_sheet_mut(&category_clone, &sheet_name) {
                    if let Some(meta_mut) = &mut sheet_mut.metadata {
                        let rule_to_save = if state.ai_general_rule_input.trim().is_empty() {
                            None
                        } else {
                            Some(state.ai_general_rule_input.trim().to_string())
                        };
                        if meta_mut.ai_general_rule != rule_to_save {
                            info!(
                                "AI General Rule updated for '{:?}/{}'. Triggering save.",
                                category_clone, sheet_name
                            );
                            meta_mut.ai_general_rule = rule_to_save;
                            meta_to_save = Some(meta_mut.clone());
                            changed = true;
                        } else { info!("AI General Rule unchanged."); }
                    }
                }
            } // Mutable borrow ends

            if changed {
                if let Some(meta) = meta_to_save {
                    let registry_immut_save = &*registry; // Immutable borrow for save
                    save_single_sheet(registry_immut_save, &meta);
                }
            }
        }
        close_popup = true; // Close after attempting save
    }

    if cancel_requested {
        close_popup = true; // Close if cancel was clicked
    }

    // Check if closed via 'X' button OR if save/cancel was requested
    if !is_window_open || close_popup {
        state.show_ai_rule_popup = false; // Update the actual state
        // Clear the input buffer on close to avoid showing stale data next time
        state.ai_general_rule_input.clear();
    }
}