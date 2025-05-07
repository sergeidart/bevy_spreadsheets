// src/ui/elements/popups/settings_popup.rs
use crate::ui::elements::editor::EditorWindowState;
use bevy::log::{error, info};
use bevy_egui::egui;
use crate::ApiKeyDisplayStatus;
use crate::SessionApiKey;
// Removed: use bevy::prelude::ResMut;

// --- MODIFIED: Function signature uses plain mutable references ---
pub fn show_settings_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    api_key_status: &mut ApiKeyDisplayStatus, // Changed from ResMut<>
    session_api_key: &mut SessionApiKey,    // Changed from ResMut<>
) {
// --- END MODIFIED ---
    if !state.show_settings_popup {
        if session_api_key.0.is_some() && api_key_status.status != "Key Set (Session)" {
            api_key_status.status = "Key Set (Session)".to_string();
        } else if session_api_key.0.is_none() && api_key_status.status != "No Key Set (Session)" {
            api_key_status.status = "No Key Set (Session)".to_string();
        }
        return;
    }
    if state.show_settings_popup {
        if session_api_key.0.is_some() {
            api_key_status.status = "Key Set (Session)".to_string();
        } else {
            api_key_status.status = "No Key Set (Session)".to_string();
        }
    }

    let mut is_window_open = state.show_settings_popup;
    let mut close_requested = false;

    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut is_window_open)
        .show(ctx, |ui| {
            ui.heading("API Key Management (Session Only)");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Current Status:");
                ui.label(api_key_status.status.as_str()); // Use directly
            });
            ui.separator();

            ui.label("Enter API Key for this session:");
            let _key_input_response = ui.add(
                egui::TextEdit::singleline(&mut state.settings_new_api_key_input)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );

            if ui.button("Set Key for Session").clicked() {
                let trimmed_key = state.settings_new_api_key_input.trim();
                if !trimmed_key.is_empty() {
                    session_api_key.0 = Some(trimmed_key.to_string()); // Use directly
                    info!("API Key set for the current session.");
                    api_key_status.status = "Key Set (Session)".to_string(); // Use directly
                    state.settings_new_api_key_input.clear();
                } else {
                    info!("API Key input was empty, not setting for session.");
                }
            }

            ui.separator();

            if ui.button("Clear Session Key").clicked() {
                if session_api_key.0.is_some() { // Use directly
                    session_api_key.0 = None; // Use directly
                    info!("API Key cleared for the current session.");
                } else {
                    info!("No API Key was set in the current session to clear.");
                }
                api_key_status.status = "No Key Set (Session)".to_string(); // Use directly
            }

            ui.separator();
             if ui.button("Close").clicked(){
                  close_requested = true;
             }
        });

    if !is_window_open || close_requested {
        state.show_settings_popup = false;
    }
}