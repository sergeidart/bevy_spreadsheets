// src/ui/elements/popups/ai_rule_popup.rs
use crate::{
    sheets::{
        definitions::SheetMetadata, // For cloning metadata
        resources::SheetRegistry,
        systems::io::save::save_single_sheet
    },
    ui::elements::editor::EditorWindowState,
};
use bevy::prelude::*;
use bevy_egui::egui;

/// Displays the modal popup window for editing the AI General Rule and parameters.
pub fn show_ai_rule_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    registry: &mut SheetRegistry, // Need mutable for saving
) {
    if !state.show_ai_rule_popup {
        return;
    }

    // Initialize input fields when popup becomes visible for the selected sheet
    if state.show_ai_rule_popup && ctx.memory(|mem| mem.is_popup_open(egui::Id::new("ai_rule_popup_window"))) {
            if let Some(sheet_name) = &state.selected_sheet_name {
                if let Some(sheet_data) = registry.get_sheet(&state.selected_category, sheet_name) {
                    if let Some(metadata) = &sheet_data.metadata {
                        state.ai_general_rule_input = metadata.ai_general_rule.clone().unwrap_or_default();
                        // Now calls public functions from sheets::definitions
                        state.ai_temperature_input = metadata.ai_temperature.unwrap_or_else(|| crate::sheets::definitions::default_temperature().unwrap_or(0.9));
                        state.ai_top_k_input = metadata.ai_top_k.unwrap_or_else(|| crate::sheets::definitions::default_top_k().unwrap_or(1));
                        state.ai_top_p_input = metadata.ai_top_p.unwrap_or_else(|| crate::sheets::definitions::default_top_p().unwrap_or(1.0));
                    }
                } else { 
                    state.ai_general_rule_input = "".to_string();
                    state.ai_temperature_input = crate::sheets::definitions::default_temperature().unwrap_or(0.9);
                    state.ai_top_k_input = crate::sheets::definitions::default_top_k().unwrap_or(1);
                    state.ai_top_p_input = crate::sheets::definitions::default_top_p().unwrap_or(1.0);
                }
            } else { 
                 state.ai_general_rule_input = "".to_string();
                 state.ai_temperature_input = crate::sheets::definitions::default_temperature().unwrap_or(0.9);
                 state.ai_top_k_input = crate::sheets::definitions::default_top_k().unwrap_or(1);
                 state.ai_top_p_input = crate::sheets::definitions::default_top_p().unwrap_or(1.0);
            }
    }


    let mut is_window_open = state.show_ai_rule_popup;
    let mut save_requested = false;
    let mut cancel_requested = false;
    let mut close_popup = false;

    egui::Window::new("Edit AI General Rule & Parameters")
        .id(egui::Id::new("ai_rule_popup_window"))
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut is_window_open)
        .show(ctx, |ui| {
            ui.label(format!(
                "Editing Rule for Sheet: '{:?}/{}'",
                state.selected_category,
                state.selected_sheet_name.as_deref().unwrap_or("?")
            ));
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
                        ui.add(egui::DragValue::new(&mut state.ai_temperature_input).speed(0.01).clamp_range(0.0..=2.0))
                            .on_hover_text("Controls randomness. Lower is more deterministic (e.g., 0.2), higher is more creative (e.g., 0.9). Default: 0.9");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Top-K:");
                        ui.add(egui::DragValue::new(&mut state.ai_top_k_input).speed(1.0).clamp_range(1..=100))
                            .on_hover_text("Considers the top K most likely tokens at each step. Set to 1 for greedy. Default: 1");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Top-P:");
                        ui.add(egui::DragValue::new(&mut state.ai_top_p_input).speed(0.01).clamp_range(0.0..=1.0))
                            .on_hover_text("Considers tokens with a cumulative probability mass of P. 1.0 means no filtering by probability. Default: 1.0");
                    });
                });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Save Settings").clicked() {
                    save_requested = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_requested = true;
                }
            });
        });

    if save_requested {
        if let Some(sheet_name_clone) = state.selected_sheet_name.clone() {
            let mut meta_to_save_cloned: Option<SheetMetadata> = None; 

            if let Some(sheet_mut) = registry.get_sheet_mut(&state.selected_category, &sheet_name_clone) {
                if let Some(meta_mut) = &mut sheet_mut.metadata {
                    let mut changed = false;
                    let rule_to_save = if state.ai_general_rule_input.trim().is_empty() { None } else { Some(state.ai_general_rule_input.trim().to_string()) };

                    if meta_mut.ai_general_rule != rule_to_save { meta_mut.ai_general_rule = rule_to_save; changed = true; }
                    if meta_mut.ai_temperature != Some(state.ai_temperature_input) { meta_mut.ai_temperature = Some(state.ai_temperature_input); changed = true; }
                    if meta_mut.ai_top_k != Some(state.ai_top_k_input) { meta_mut.ai_top_k = Some(state.ai_top_k_input); changed = true; }
                    if meta_mut.ai_top_p != Some(state.ai_top_p_input) { meta_mut.ai_top_p = Some(state.ai_top_p_input); changed = true; }

                    if changed {
                        info!(
                            "AI Rule/Parameters updated for '{:?}/{}'. Triggering save.",
                            state.selected_category, sheet_name_clone
                        );
                        meta_to_save_cloned = Some(meta_mut.clone()); 
                    } else {
                        info!("AI Rule/Parameters unchanged for '{:?}/{}'.", state.selected_category, sheet_name_clone);
                    }
                }
            }
            if let Some(meta_for_saving) = meta_to_save_cloned {
                let registry_immut_save = &*registry; 
                save_single_sheet(registry_immut_save, &meta_for_saving);
            }
        }
        close_popup = true;
    }

    if cancel_requested {
        close_popup = true;
    }

    if !is_window_open || close_popup {
        state.show_ai_rule_popup = false;
    }
}