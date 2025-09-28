// src/ui/elements/popups/ai_rule_popup.rs
use crate::{
    sheets::{
        definitions::{default_ai_model_id, SheetMetadata},
        resources::SheetRegistry,
        systems::io::save::save_single_sheet,
    },
    ui::elements::editor::EditorWindowState,
};
use bevy::prelude::*;
use bevy_egui::egui;

/// Displays the modal popup window for editing the AI Model ID, General Rule, and parameters.
pub fn show_ai_rule_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry,
) {
    if !state.show_ai_rule_popup {
        return;
    }

    // --- Simplified Initialization Logic ---
    if state.ai_rule_popup_needs_init {
        if let Some(sheet_name) = &state.selected_sheet_name {
            if let Some(sheet_data) = registry.get_sheet(&state.selected_category, sheet_name) {
                if let Some(metadata) = &sheet_data.metadata {
                    info!(
                        "Initializing AI Config popup for sheet: '{:?}/{}'",
                        state.selected_category, sheet_name
                    );
                    state.ai_model_id_input = metadata.ai_model_id.clone();
                    state.ai_general_rule_input =
                        metadata.ai_general_rule.clone().unwrap_or_default();
                    state.ai_rule_popup_grounding = Some(
                        metadata
                            .requested_grounding_with_google_search
                            .unwrap_or(false),
                    );
                } else {
                    warn!("Metadata not found for sheet '{:?}/{}' during AI Config popup init. Using defaults.", state.selected_category, sheet_name);
                    state.ai_model_id_input = default_ai_model_id();
                    state.ai_general_rule_input = "".to_string();
                    state.ai_rule_popup_grounding = Some(false);
                }
            } else {
                warn!(
                    "Sheet '{:?}/{}' not found during AI Config popup init. Using defaults.",
                    state.selected_category, sheet_name
                );
                state.ai_model_id_input = default_ai_model_id();
                state.ai_general_rule_input = "".to_string();
                state.ai_rule_popup_grounding = Some(false);
            }
        } else {
            info!("No sheet selected for AI Config popup. Using defaults.");
            state.ai_model_id_input = default_ai_model_id();
            state.ai_general_rule_input = "".to_string();
            state.ai_rule_popup_grounding = Some(false);
        }
        state.ai_rule_popup_needs_init = false; // Consumed the init flag
    }
    // --- END MODIFIED ---

    // If selection changed while open, reinitialize instead of closing
    if let (Some(last_cat_opt), Some(last_sheet)) = (
        &state.ai_rule_popup_last_category,
        &state.ai_rule_popup_last_sheet,
    ) {
        if &state.selected_category != last_cat_opt
            || state.selected_sheet_name.as_deref() != Some(last_sheet)
        {
            state.ai_rule_popup_needs_init = true;
            state.ai_rule_popup_last_category = Some(state.selected_category.clone());
            state.ai_rule_popup_last_sheet = state.selected_sheet_name.clone();
        }
    } else if state.show_ai_rule_popup {
        state.ai_rule_popup_last_category = Some(state.selected_category.clone());
        state.ai_rule_popup_last_sheet = state.selected_sheet_name.clone();
    }

    let mut is_window_open = state.show_ai_rule_popup;
    let mut save_requested = false;
    let mut cancel_requested = false;
    let mut close_popup = false;

    egui::Window::new("AI Context")
        .id(egui::Id::new("ai_rule_popup_window")) // Keep ID for memory
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut is_window_open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("AI Model:");
                // The TextEdit for ai_model_id_input. No validation here.
                ui.add(
                    egui::TextEdit::singleline(&mut state.ai_model_id_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("e.g., gemini-1.5-flash, gemini-2.5-flash-preview-04-17"),
                );
            });
            ui.separator();
            ui.label("Sheet AI Context");
            ui.add_sized(
                [ui.available_width(), 100.0],
                egui::TextEdit::multiline(&mut state.ai_general_rule_input)
                    .hint_text(
                        "Provide a general instruction or context for the AI processing this sheet",
                    )
                    .desired_rows(5),
            );
            ui.separator();
            // Row: AI options toggles (stacked vertically)
            ui.vertical(|ui_v| {
                let mut grounded = state.ai_rule_popup_grounding.unwrap_or(false);
                if ui_v
                    .checkbox(&mut grounded, "Search")
                    .on_hover_text("Enable Google Search grounding for AI responses")
                    .changed()
                {
                    state.ai_rule_popup_grounding = Some(grounded);
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                // Enable save if a sheet is actually selected
                if ui
                    .add_enabled(
                        state.selected_sheet_name.is_some(),
                        egui::Button::new("Save Settings"),
                    )
                    .clicked()
                {
                    save_requested = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_requested = true;
                }
            });
        });

    if save_requested {
        if let Some(sheet_name_clone) = state.selected_sheet_name.clone() {
            // Ensure a sheet is selected
            let mut meta_to_save_cloned: Option<SheetMetadata> = None;

            if let Some(sheet_mut) =
                registry.get_sheet_mut(&state.selected_category, &sheet_name_clone)
            {
                if let Some(meta_mut) = &mut sheet_mut.metadata {
                    let mut changed = false;

                    let model_id_input_trimmed = state.ai_model_id_input.trim();
                    let final_model_id_to_save = if model_id_input_trimmed.is_empty() {
                        default_ai_model_id() // Revert to default if input is cleared
                    } else {
                        model_id_input_trimmed.to_string()
                    };

                    if meta_mut.ai_model_id != final_model_id_to_save {
                        meta_mut.ai_model_id = final_model_id_to_save;
                        changed = true;
                    }

                    let rule_to_save = if state.ai_general_rule_input.trim().is_empty() {
                        None
                    } else {
                        Some(state.ai_general_rule_input.trim().to_string())
                    };
                    if meta_mut.ai_general_rule != rule_to_save {
                        meta_mut.ai_general_rule = rule_to_save;
                        changed = true;
                    }

                    // Persist Grounding toggle
                    if let Some(ground) = state.ai_rule_popup_grounding {
                        if meta_mut.requested_grounding_with_google_search != Some(ground) {
                            meta_mut.requested_grounding_with_google_search = Some(ground);
                            changed = true;
                        }
                    }

                    if changed {
                        info!(
                            "AI Config updated for '{:?}/{}': Model ID='{}', Rule='{:?}', AddRows={}, Grounding={:?}. Triggering save.",
                            state.selected_category, sheet_name_clone,
                            meta_mut.ai_model_id, meta_mut.ai_general_rule,
                            meta_mut.ai_enable_row_generation, meta_mut.requested_grounding_with_google_search
                        );
                        meta_to_save_cloned = Some(meta_mut.clone());
                    } else {
                        info!(
                            "AI Config unchanged for '{:?}/{}'.",
                            state.selected_category, sheet_name_clone
                        );
                    }
                }
            }
            if let Some(meta_for_saving) = meta_to_save_cloned {
                let registry_immut_save = &*registry;
                save_single_sheet(registry_immut_save, &meta_for_saving);
            }
        } else {
            warn!("Save AI Config requested, but no sheet is selected.");
        }
        close_popup = true;
    }

    if cancel_requested {
        close_popup = true;
    }

    // If the window is closed via 'x' or by logic above
    if !is_window_open || close_popup {
        state.show_ai_rule_popup = false;
        state.ai_rule_popup_needs_init = true; // Set to true so it re-initializes next time it's opened
    }
}
