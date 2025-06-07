// src/ui/elements/popups/settings_popup.rs
use crate::ui::elements::editor::EditorWindowState;
use bevy::log::info;
use bevy_egui::egui;
use crate::ApiKeyDisplayStatus;
use crate::SessionApiKey;
// Removed: use bevy::prelude::ResMut;
use whoami;

// --- MODIFIED: Function signature uses plain mutable references ---
pub fn show_settings_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    api_key_status: &mut ApiKeyDisplayStatus, // Changed from ResMut<>
    session_api_key: &mut SessionApiKey,    // Changed from ResMut<>
) {
// --- END MODIFIED ---
    if state.show_settings_popup {
        // Only check keyring when popup is first opened
        let popup_just_opened = state.show_settings_popup && !state.was_settings_popup_open;
        if popup_just_opened {
            let username = whoami::username();
            info!("[DEBUG] (Popup just opened) Checking key status for username: {username}");
            let keyring_status = keyring::Entry::new("GoogleGeminiAPI", username.as_str())
                .and_then(|entry| entry.get_password());
            match &keyring_status {
                Ok(loaded_key) => info!("[DEBUG] (Popup just opened) Loaded key from keyring: '{}'", loaded_key),
                Err(e) => info!("[DEBUG] (Popup just opened) Failed to load key from keyring: {e}"),
            }
            if let Ok(loaded_key) = keyring_status {
                if !loaded_key.is_empty() {
                    session_api_key.0 = Some(loaded_key);
                    api_key_status.status = "Key Set".to_string();
                } else {
                    session_api_key.0 = None;
                    api_key_status.status = "No Key Set".to_string();
                }
            } else {
                session_api_key.0 = None;
                api_key_status.status = "No Key Set".to_string();
            }
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
            ui.heading("API Key Management");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Current Status:");
                ui.label(api_key_status.status.as_str()); // Use directly
            });
            ui.separator();

            ui.label("Enter API Key:");
            let _key_input_response = ui.add(
                egui::TextEdit::singleline(&mut state.settings_new_api_key_input)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );

            if ui.button("Set Key").clicked() {
                let username = whoami::username();
                info!("[DEBUG] Saving key for username: {username}");
                // Clone the input before any mutable borrow
                let trimmed_key_owned = state.settings_new_api_key_input.trim().to_string();
                let trimmed_key = trimmed_key_owned.as_str();
                if !trimmed_key.is_empty() {
                    // Save to Windows Credential Manager for Python access using keyring
                    match keyring::Entry::new("GoogleGeminiAPI", username.as_str())
                        .and_then(|entry| entry.set_password(trimmed_key))
                    {
                        Ok(_) => {
                            info!("API Key saved to Windows Credential Manager for cross-language access.");
                            // Try to read it back immediately
                            match keyring::Entry::new("GoogleGeminiAPI", username.as_str())
                                .and_then(|entry| entry.get_password())
                            {
                                Ok(loaded_key) => info!("[DEBUG] After save, loaded key: '{}', len={}", loaded_key, loaded_key.len()),
                                Err(e) => info!("[DEBUG] After save, failed to load key: {e}"),
                            }
                            session_api_key.0 = Some(trimmed_key.to_string());
                            api_key_status.status = "Key Set".to_string();
                            state.settings_new_api_key_input.clear();
                        }
                        Err(e) => {
                            info!("Failed to save API Key to Windows Credential Manager: {e}");
                            session_api_key.0 = None;
                            api_key_status.status = "No Key Set".to_string();
                        }
                    }
                } else {
                    info!("API Key input was empty.");
                }
            }

            ui.separator();

            if ui.button("Clear Key").clicked() {
                let username = whoami::username();
                info!("[DEBUG] Clearing key for username: {username}");
                session_api_key.0 = None;
                info!("API Key cleared from session.");
                match keyring::Entry::new("GoogleGeminiAPI", username.as_str()) {
                    Ok(entry) => {
                        match entry.delete_credential() {
                            Ok(_) => info!("API Key removed from Windows Credential Manager."),
                            Err(e) => info!("Failed to remove API Key from Windows Credential Manager: {e}"),
                        }
                    }
                    Err(e) => info!("Failed to open keyring entry for deletion: {e}"),
                }
                // Try to read it back to confirm deletion
                match keyring::Entry::new("GoogleGeminiAPI", username.as_str())
                    .and_then(|entry| entry.get_password())
                {
                    Ok(loaded_key) => info!("[DEBUG] After clear, loaded key: '{}', len={}", loaded_key, loaded_key.len()),
                    Err(e) => info!("[DEBUG] After clear, failed to load key: {e}"),
                }
                api_key_status.status = "No Key Set".to_string();
            }

            ui.separator();
             if ui.button("Close").clicked(){
                  close_requested = true;
             }
        });

    if !is_window_open || close_requested {
        state.show_settings_popup = false;
    }
    // At the end of the function, update the tracker
    state.was_settings_popup_open = state.show_settings_popup;
}