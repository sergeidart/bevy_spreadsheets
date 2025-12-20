// src/ui/elements/popups/settings_popup.rs
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::EditorWindowState;
use crate::ApiKeyDisplayStatus;
use crate::SessionApiKey;
use bevy::log::info;
use bevy_egui::egui;
// Removed: use bevy::prelude::ResMut;
use crate::settings::io::{load_settings_from_file, save_settings_to_file};
use crate::settings::AppSettings;
use crate::visual_copier::events::{
    PickFolderRequest, QueueTopPanelCopyEvent, ReverseTopPanelFoldersEvent,
    VisualCopierStateChanged,
};
use crate::visual_copier::resources::VisualCopierManager;
use bevy::prelude::EventWriter;
use whoami;

// --- MODIFIED: Function signature uses plain mutable references ---
pub fn show_settings_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    api_key_status: &mut ApiKeyDisplayStatus, // Changed from ResMut<>
    session_api_key: &mut SessionApiKey,      // Changed from ResMut<>
    // Registry retained for potential future settings, currently unused
    _registry: &mut SheetRegistry,
    // NEW: Quick Copy settings (Copy on Exit)
    _copier_manager: &mut VisualCopierManager,
    // Event writers to drive Quick Copy actions inside Settings
    _pick_folder_writer: &mut EventWriter<PickFolderRequest>,
    _queue_top_panel_copy_writer: &mut EventWriter<QueueTopPanelCopyEvent>,
    _reverse_folders_writer: &mut EventWriter<ReverseTopPanelFoldersEvent>,
    _state_changed_writer: &mut EventWriter<VisualCopierStateChanged>,
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
                Ok(loaded_key) => info!(
                    "[DEBUG] (Popup just opened) Loaded key from keyring: '{}'",
                    loaded_key
                ),
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
            // Also load persisted AppSettings into the UI state (best-effort)
            if let Ok(loaded) = load_settings_from_file::<AppSettings>() {
                state.fps_setting = loaded.fps_setting;
                state.show_hidden_sheets = loaded.show_hidden_sheets;
                state.ai_depth_limit = loaded.ai_depth_limit;
                state.ai_width_limit = loaded.ai_width_limit;
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
            ui.horizontal_wrapped(|ui_h| {
                ui_h.label("API Key:");
                let _key_input_response = ui_h.add(
                    egui::TextEdit::singleline(&mut state.settings_new_api_key_input)
                        .password(true)
                        .desired_width(280.0),
                );
                ui_h.label(api_key_status.status.as_str());
                if ui_h.button("Set Key").clicked() {
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
                ui_h.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui_r| {
                    if ui_r.button("Clear Key").clicked() {
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
                });
            });
            ui.separator();
                    ui.heading("Performance");
                    ui.horizontal_wrapped(|ui_h| {
                        ui_h.label("Frame rate:");
                        // Dropdown selector for FPS
                        let mut fps_choice = state.fps_setting;
                        egui::ComboBox::from_label("")
                            .selected_text(match fps_choice {
                                crate::ui::elements::editor::state::FpsSetting::Thirty => "30",
                                crate::ui::elements::editor::state::FpsSetting::Sixty => "60",
                                crate::ui::elements::editor::state::FpsSetting::ScreenHz => "Screen Hz (Auto)",
                            })
                            .show_ui(ui_h, |ui_cb| {
                                ui_cb.selectable_value(&mut fps_choice, crate::ui::elements::editor::state::FpsSetting::Thirty, "30");
                                ui_cb.selectable_value(&mut fps_choice, crate::ui::elements::editor::state::FpsSetting::Sixty, "60");
                                ui_cb.selectable_value(&mut fps_choice, crate::ui::elements::editor::state::FpsSetting::ScreenHz, "Screen Hz (Auto)");
                            });
                        if fps_choice != state.fps_setting {
                            state.fps_setting = fps_choice;
                            // Persist the change (and current structure sheets toggle)
                            let settings_to_save = AppSettings { fps_setting: state.fps_setting, show_hidden_sheets: state.show_hidden_sheets, ai_depth_limit: state.ai_depth_limit, ai_width_limit: state.ai_width_limit };
                            if let Err(e) = save_settings_to_file(&settings_to_save) {
                                info!("Failed to save AppSettings: {}", e);
                            }
                        }
                    });
            ui.separator();
            ui.heading("Database Views");
            ui.horizontal_wrapped(|ui_h| {
                let mut show_hidden = state.show_hidden_sheets;
                if ui_h.checkbox(&mut show_hidden, "Show hidden sheets").on_hover_text("Temporarily show all sheets regardless of their hidden flag").changed() {
                    state.show_hidden_sheets = show_hidden;
                    // Persist both settings together
                    let settings_to_save = AppSettings { fps_setting: state.fps_setting, show_hidden_sheets: state.show_hidden_sheets, ai_depth_limit: state.ai_depth_limit, ai_width_limit: state.ai_width_limit };
                    if let Err(e) = save_settings_to_file(&settings_to_save) {
                        info!("Failed to save AppSettings: {}", e);
                    }
                }
                    });
            ui.separator();
            ui.heading("AI Settings");
            ui.horizontal_wrapped(|ui_h| {
                ui_h.label("Depth limit:");
                let mut depth = state.ai_depth_limit;
                let depth_drag = egui::DragValue::new(&mut depth).range(1..=10).speed(0.1);
                if ui_h.add(depth_drag).on_hover_text("How many levels of structure tables to process (default: 2)").changed() {
                    state.ai_depth_limit = depth;
                    let settings_to_save = AppSettings { fps_setting: state.fps_setting, show_hidden_sheets: state.show_hidden_sheets, ai_depth_limit: state.ai_depth_limit, ai_width_limit: state.ai_width_limit };
                    if let Err(e) = save_settings_to_file(&settings_to_save) {
                        info!("Failed to save AppSettings: {}", e);
                    }
                }
            });
            ui.horizontal_wrapped(|ui_h| {
                ui_h.label("Width limit (rows per batch):");
                let mut width = state.ai_width_limit;
                let width_drag = egui::DragValue::new(&mut width).range(1..=256).speed(1.0);
                if ui_h.add(width_drag).on_hover_text("How many rows to send in one AI batch (default: 32)").changed() {
                    state.ai_width_limit = width;
                    let settings_to_save = AppSettings { fps_setting: state.fps_setting, show_hidden_sheets: state.show_hidden_sheets, ai_depth_limit: state.ai_depth_limit, ai_width_limit: state.ai_width_limit };
                    if let Err(e) = save_settings_to_file(&settings_to_save) {
                        info!("Failed to save AppSettings: {}", e);
                    }
                }
            });
            // Quick Copy section hidden in DB-focused mode.
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
