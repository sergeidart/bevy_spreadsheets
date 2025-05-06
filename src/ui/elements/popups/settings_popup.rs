// src/ui/elements/popups/settings_popup.rs
use crate::{
    ui::elements::editor::EditorWindowState, KEYRING_API_KEY_USERNAME,
    KEYRING_SERVICE_NAME, // Import constants
};
use bevy::log::{error, info}; // Use bevy logging
use bevy_egui::egui;
// No bevy::prelude needed if only using logging macros

/// Displays the modal popup window for managing the API Key.
pub fn show_settings_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    // We might need EventWriter<SheetOperationFeedback> later for feedback
) {
    if !state.show_settings_popup {
        return;
    }

    let mut is_window_open = state.show_settings_popup; // Local copy for window open state
    let mut close_requested = false; // Flag to signal closure from within

    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut is_window_open) // Pass mutable local copy here
        .show(ctx, |ui| {
            ui.heading("API Key Management");
            ui.separator();

            // --- Status Display ---
            ui.horizontal(|ui| {
                ui.label("Current Status:");
                ui.label(state.settings_api_key_status.as_str());
            });
            ui.separator();

            // --- Input & Save ---
            ui.label("Enter New API Key:");
            let _key_input_response = ui.add( // Store response if needed
                egui::TextEdit::singleline(&mut state.settings_new_api_key_input)
                    .password(true)
                    .desired_width(f32::INFINITY), // Fill width
            );

            if ui.button("Save Key").clicked() {
                let trimmed_key = state.settings_new_api_key_input.trim();
                if !trimmed_key.is_empty() {
                    match keyring::Entry::new(
                        KEYRING_SERVICE_NAME,
                        KEYRING_API_KEY_USERNAME,
                    ) {
                        Ok(entry) => match entry.set_password(trimmed_key) {
                            Ok(_) => {
                                info!("API Key saved successfully.");
                                state.settings_api_key_status = "Key Set".to_string();
                                state.settings_new_api_key_input.clear();
                                // TODO: Send feedback event
                            }
                            Err(e) => {
                                error!("Failed to save API key: {}", e);
                                state.settings_api_key_status = "Error Saving".to_string();
                                // TODO: Send feedback event
                            }
                        },
                        Err(e) => {
                            error!("Failed to create keyring entry: {}", e);
                            state.settings_api_key_status = "Keyring Error".to_string();
                            // TODO: Send feedback event
                        }
                    }
                } else {
                    info!("API Key input was empty, not saving.");
                }
            }

            ui.separator();

            // --- Remove Key ---
            if ui.button("Remove Saved Key").clicked() {
                 match keyring::Entry::new(
                    KEYRING_SERVICE_NAME,
                    KEYRING_API_KEY_USERNAME,
                ) {
                    Ok(entry) => {
                        match entry.delete_credential() { // Use delete_credential
                            Ok(_) => {
                                info!("API Key removed successfully.");
                                state.settings_api_key_status = "No Key Set".to_string();
                                // TODO: Send feedback event
                            }
                            Err(keyring::Error::NoEntry) => {
                                 info!("No API Key was set, nothing to remove.");
                                 state.settings_api_key_status = "No Key Set".to_string();
                            }
                            Err(e) => {
                                error!("Failed to remove API key: {}", e);
                                state.settings_api_key_status = "Error Removing".to_string();
                                // TODO: Send feedback event
                            }
                         }
                    }
                     Err(e) => {
                        error!("Failed to create keyring entry for removal: {}", e);
                        state.settings_api_key_status = "Keyring Error".to_string();
                        // TODO: Send feedback event
                    }
                }
            }

            ui.separator();
            // Add a close button explicitly
             if ui.button("Close").clicked(){
                  close_requested = true; // Signal intent to close
             }

        }); // End window.show - Borrow on is_window_open released

    // Update state if window was closed via 'X' or explicit Close button
    if !is_window_open || close_requested {
        state.show_settings_popup = false; // Update actual state
        // Clear sensitive input when closing
        state.settings_new_api_key_input.clear();
    }
}