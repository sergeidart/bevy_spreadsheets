// src/ui/elements/popups/settings_popup.rs
use crate::ui::elements::editor::EditorWindowState;
use bevy::log::info;
use bevy_egui::egui;
use crate::ApiKeyDisplayStatus;
use crate::SessionApiKey;
use crate::sheets::resources::SheetRegistry;
// Removed: use bevy::prelude::ResMut;
use whoami;
use crate::settings::io::{save_settings_to_file, load_settings_from_file};
use crate::settings::AppSettings;
use crate::visual_copier::resources::VisualCopierManager;
use bevy::prelude::EventWriter;
use crate::visual_copier::events::{PickFolderRequest, QueueTopPanelCopyEvent, ReverseTopPanelFoldersEvent, VisualCopierStateChanged};

// --- MODIFIED: Function signature uses plain mutable references ---
pub fn show_settings_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    api_key_status: &mut ApiKeyDisplayStatus, // Changed from ResMut<>
    session_api_key: &mut SessionApiKey,    // Changed from ResMut<>
    // Registry retained for potential future settings, currently unused
    _registry: &mut SheetRegistry,
    // NEW: Quick Copy settings (Copy on Exit)
    copier_manager: &mut VisualCopierManager,
    // Event writers to drive Quick Copy actions inside Settings
    pick_folder_writer: &mut EventWriter<PickFolderRequest>,
    queue_top_panel_copy_writer: &mut EventWriter<QueueTopPanelCopyEvent>,
    reverse_folders_writer: &mut EventWriter<ReverseTopPanelFoldersEvent>,
    state_changed_writer: &mut EventWriter<VisualCopierStateChanged>,
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
                // Also load persisted AppSettings into the UI state (best-effort)
                if let Ok(loaded) = load_settings_from_file::<AppSettings>() {
                    state.fps_setting = loaded.fps_setting;
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
                            // Persist the new choice
                            let settings_to_save = AppSettings { fps_setting: state.fps_setting };
                            if let Err(e) = save_settings_to_file(&settings_to_save) {
                                info!("Failed to save AppSettings: {}", e);
                            }
                        }
                    });
            ui.heading("Quick Copy");
            // Row 1: FROM
            ui.horizontal_wrapped(|ui_h| {
                ui_h.label("FROM");
                if ui_h.button("Pick...").on_hover_text("Select source folder").clicked() {
                    pick_folder_writer.write(PickFolderRequest { for_task_id: None, is_start_folder: true });
                }
                let from_path_str = copier_manager.top_panel_from_folder.as_ref().map_or("None".to_string(), |p| p.display().to_string());
                ui_h.monospace(from_path_str);
            });
            // Row 2: TO
            ui.horizontal_wrapped(|ui_h| {
                ui_h.label("TO");
                if ui_h.button("Pick...").on_hover_text("Select destination folder").clicked() {
                    pick_folder_writer.write(PickFolderRequest { for_task_id: None, is_start_folder: false });
                }
                let to_path_str = copier_manager.top_panel_to_folder.as_ref().map_or("None".to_string(), |p| p.display().to_string());
                ui_h.monospace(to_path_str);
            });
            // Row 3: actions
            ui.horizontal_wrapped(|ui_h| {
                if ui_h.button("Swap ↔").clicked() { reverse_folders_writer.write(ReverseTopPanelFoldersEvent); }
                let can_copy = copier_manager.top_panel_from_folder.is_some() && copier_manager.top_panel_to_folder.is_some();
                if ui_h.add_enabled(can_copy, egui::Button::new("COPY")).clicked() {
                    queue_top_panel_copy_writer.write(QueueTopPanelCopyEvent);
                }
                ui_h.label(&copier_manager.top_panel_copy_status);
                let mut copy_on_exit = copier_manager.copy_top_panel_on_exit;
                if ui_h.checkbox(&mut copy_on_exit, "Copy on Exit").on_hover_text("Perform this Quick Copy operation synchronously on App Exit.").changed() {
                    copier_manager.copy_top_panel_on_exit = copy_on_exit;
                    state_changed_writer.write(VisualCopierStateChanged);
                }
            });
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