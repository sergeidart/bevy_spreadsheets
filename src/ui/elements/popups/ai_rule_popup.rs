// src/ui/elements/popups/ai_rule_popup.rs
use crate::{
    sheets::{
        definitions::{SheetMetadata, default_ai_model_id, default_temperature, default_top_k, default_top_p}, // Added defaults
        resources::SheetRegistry,
        systems::io::save::save_single_sheet
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

    // --- MODIFIED: Robust Initialization Logic ---
    if state.ai_rule_popup_needs_init {
        if let Some(sheet_name) = &state.selected_sheet_name {
            if let Some(sheet_data) = registry.get_sheet(&state.selected_category, sheet_name) {
                if let Some(metadata) = &sheet_data.metadata {
                    info!("Initializing AI Config popup for sheet: '{:?}/{}'", state.selected_category, sheet_name);
                    state.ai_model_id_input = metadata.ai_model_id.clone();
                    state.ai_general_rule_input = metadata.ai_general_rule.clone().unwrap_or_default();
                    state.ai_temperature_input = metadata.ai_temperature.unwrap_or_else(|| default_temperature().unwrap_or(0.9));
                    state.ai_top_k_input = metadata.ai_top_k.unwrap_or_else(|| default_top_k().unwrap_or(1));
                    state.ai_top_p_input = metadata.ai_top_p.unwrap_or_else(|| default_top_p().unwrap_or(1.0));
                } else {
                    warn!("Metadata not found for sheet '{:?}/{}' during AI Config popup init. Using defaults.", state.selected_category, sheet_name);
                    state.ai_model_id_input = default_ai_model_id();
                    state.ai_general_rule_input = "".to_string();
                    state.ai_temperature_input = default_temperature().unwrap_or(0.9);
                    state.ai_top_k_input = default_top_k().unwrap_or(1);
                    state.ai_top_p_input = default_top_p().unwrap_or(1.0);
                }
            } else {
                warn!("Sheet '{:?}/{}' not found during AI Config popup init. Using defaults.", state.selected_category, sheet_name);
                state.ai_model_id_input = default_ai_model_id();
                state.ai_general_rule_input = "".to_string();
                state.ai_temperature_input = default_temperature().unwrap_or(0.9);
                state.ai_top_k_input = default_top_k().unwrap_or(1);
                state.ai_top_p_input = default_top_p().unwrap_or(1.0);
            }
        } else {
            info!("No sheet selected for AI Config popup. Using defaults.");
            state.ai_model_id_input = default_ai_model_id();
            state.ai_general_rule_input = "".to_string();
            state.ai_temperature_input = default_temperature().unwrap_or(0.9);
            state.ai_top_k_input = default_top_k().unwrap_or(1);
            state.ai_top_p_input = default_top_p().unwrap_or(1.0);
        }
        state.ai_rule_popup_needs_init = false; // Consumed the init flag
    }
    // --- END MODIFIED ---

    // --- ADDITION: Close popup if context changed ---
    if let (Some(opened_cat), Some(opened_sheet)) = (state.selected_category.clone(), state.selected_sheet_name.clone()) {
        static mut LAST_POPUP_CATEGORY: Option<String> = None;
        static mut LAST_POPUP_SHEET: Option<String> = None;
        let mut context_changed = false;
        unsafe {
            if state.show_ai_rule_popup {
                if let (Some(last_cat), Some(last_sheet)) = (&LAST_POPUP_CATEGORY, &LAST_POPUP_SHEET) {
                    if last_cat != &opened_cat || last_sheet != &opened_sheet {
                        context_changed = true;
                    }
                } else {
                    LAST_POPUP_CATEGORY = Some(opened_cat.clone());
                    LAST_POPUP_SHEET = Some(opened_sheet.clone());
                }
            } else {
                LAST_POPUP_CATEGORY = None;
                LAST_POPUP_SHEET = None;
            }
        }
        if context_changed {
            state.show_ai_rule_popup = false;
            return;
        }
    }

    let mut is_window_open = state.show_ai_rule_popup;
    let mut save_requested = false;
    let mut cancel_requested = false;
    let mut close_popup = false;

    egui::Window::new("Edit AI Model & Parameters")
        .id(egui::Id::new("ai_rule_popup_window")) // Keep ID for memory
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut is_window_open)
        .show(ctx, |ui| {
            ui.label(format!(
                "Editing AI Config for Sheet: '{:?}/{}'",
                state.selected_category,
                state.selected_sheet_name.as_deref().unwrap_or("?")
            ));
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("AI Model ID:");
                // The TextEdit for ai_model_id_input. No validation here.
                ui.add(
                    egui::TextEdit::singleline(&mut state.ai_model_id_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("e.g., gemini-1.5-flash, gemini-2.5-flash-preview-04-17"),
                );
            });
            ui.small("Specify the AI model identifier (e.g., 'gemini-1.5-flash').");
            ui.separator();

            ui.label("Provide a general instruction or context for the AI processing this sheet:");
            ui.add_sized(
                [ui.available_width(), 100.0],
                egui::TextEdit::multiline(&mut state.ai_general_rule_input)
                    .hint_text("e.g., 'Correct spelling and grammar. Output as JSON array of strings.'")
                    .desired_rows(5),
            );

            ui.separator();
            egui::CollapsingHeader::new("⚙️ Advanced Generation Settings")
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Temperature:");
                        ui.add(egui::DragValue::new(&mut state.ai_temperature_input).speed(0.01).range(0.0..=2.0))
                            .on_hover_text("Controls randomness. Lower is more deterministic (e.g., 0.2), higher is more creative (e.g., 0.9). Default: 0.9");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Top-K:");
                        ui.add(egui::DragValue::new(&mut state.ai_top_k_input).speed(1.0).range(1..=100))
                            .on_hover_text("Considers the top K most likely tokens at each step. Set to 1 for greedy. Default: 1");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Top-P:");
                        ui.add(egui::DragValue::new(&mut state.ai_top_p_input).speed(0.01).range(0.0..=1.0))
                            .on_hover_text("Considers tokens with a cumulative probability mass of P. 1.0 means no filtering by probability. Default: 1.0");
                    });
                });
            ui.separator();
            ui.horizontal(|ui| {
                // Enable save if a sheet is actually selected
                if ui.add_enabled(state.selected_sheet_name.is_some(), egui::Button::new("Save Settings")).clicked() {
                    save_requested = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_requested = true;
                }
            });
        });

    if save_requested {
        if let Some(sheet_name_clone) = state.selected_sheet_name.clone() { // Ensure a sheet is selected
            let mut meta_to_save_cloned: Option<SheetMetadata> = None;

            if let Some(sheet_mut) = registry.get_sheet_mut(&state.selected_category, &sheet_name_clone) {
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

                    let rule_to_save = if state.ai_general_rule_input.trim().is_empty() { None } else { Some(state.ai_general_rule_input.trim().to_string()) };
                    if meta_mut.ai_general_rule != rule_to_save { meta_mut.ai_general_rule = rule_to_save; changed = true; }

                    const EPSILON: f32 = 0.001;
                    // Temperature
                    let new_temp_opt = Some(state.ai_temperature_input); // Assuming UI always provides a value
                    if meta_mut.ai_temperature.map_or(true, |val| (val - state.ai_temperature_input).abs() > EPSILON) || meta_mut.ai_temperature.is_none() {
                        meta_mut.ai_temperature = new_temp_opt;
                        changed = true;
                    }

                    // Top-K
                    let new_top_k_opt = Some(state.ai_top_k_input);
                    if meta_mut.ai_top_k != new_top_k_opt {
                        meta_mut.ai_top_k = new_top_k_opt;
                        changed = true;
                    }

                    // Top-P
                    let new_top_p_opt = Some(state.ai_top_p_input);
                     if meta_mut.ai_top_p.map_or(true, |val| (val - state.ai_top_p_input).abs() > EPSILON) || meta_mut.ai_top_p.is_none() {
                        meta_mut.ai_top_p = new_top_p_opt;
                        changed = true;
                    }

                    if changed {
                        info!(
                            "AI Config updated for '{:?}/{}': Model ID='{}', Rule='{:?}', Temp={:?}, TopK={:?}, TopP={:?}. Triggering save.",
                            state.selected_category, sheet_name_clone,
                            meta_mut.ai_model_id, meta_mut.ai_general_rule, meta_mut.ai_temperature, meta_mut.ai_top_k, meta_mut.ai_top_p
                        );
                        meta_to_save_cloned = Some(meta_mut.clone());
                    } else {
                        info!("AI Config unchanged for '{:?}/{}'.", state.selected_category, sheet_name_clone);
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