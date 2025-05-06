// src/main.rs

use bevy::{log::Level, prelude::*};
use bevy_egui::EguiPlugin;
use bevy_tokio_tasks::TokioTasksPlugin; // <-- ADDED

// Import plugins defined within this app
mod sheets; // Core data logic
mod ui;     // Editor UI
mod example_definitions; // Holds the metadata for sheets this app edits

use sheets::SheetsPlugin;
use ui::EditorUiPlugin;

// --- Keyring Constants ---
const KEYRING_SERVICE_NAME: &str = "bevy_spreadsheet_ai";
const KEYRING_API_KEY_USERNAME: &str = "llm_api_key";

fn main() {
    App::new()
        // --- Core Bevy Plugins ---
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Standalone Sheet Editor".into(),
                    // resolution: (1280., 720.).into(), // Optional
                    ..default()
                }),
                ..default()
            }),
        )
        // --- Essential Third-Party Plugins ---
        .add_plugins(EguiPlugin {
            enable_multipass_for_primary_context: true,
        })
        .add_plugins(TokioTasksPlugin::default()) // <-- ADDED Async Plugin

        // --- Add Application Plugins ---
        .add_plugins(SheetsPlugin)
        .add_plugins(EditorUiPlugin)

        // --- Setup System ---
        // Add a system to check keyring status on startup
        .add_systems(Startup, check_api_key_startup)

        // --- Run ---
        .run();
}

// System to initialize API key status display
fn check_api_key_startup(mut state: Local<ui::elements::editor::EditorWindowState>) {
     match keyring::Entry::new(KEYRING_SERVICE_NAME, KEYRING_API_KEY_USERNAME) {
        Ok(entry) => match entry.get_password() {
             Ok(_) => {
                  // Don't log the key itself!
                  info!("API Key found in keyring on startup.");
                  state.settings_api_key_status = "Key Set".to_string();
             },
             Err(keyring::Error::NoEntry) => {
                  info!("No API Key found in keyring on startup.");
                  state.settings_api_key_status = "No Key Set".to_string();
             }
             Err(e) => {
                  error!("Error accessing keyring on startup: {}", e);
                  state.settings_api_key_status = "Keyring Error".to_string();
             }
        },
        Err(e) => {
             error!("Error creating keyring entry on startup: {}", e);
             state.settings_api_key_status = "Keyring Error".to_string();
        }
   }
}