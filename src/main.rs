// spreadsheet_app/src/main.rs

use bevy::{log::Level, prelude::*};
use bevy_egui::EguiPlugin;

// Import plugins defined within this app
mod sheets; // Core data logic
mod ui;     // Editor UI
mod example_definitions; // Holds the metadata for sheets this app edits

use sheets::SheetsPlugin;
use ui::EditorUiPlugin;

fn main() {
    App::new()
        // --- Core Bevy Plugins ---
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Standalone Sheet Editor".into(),
                    // resolution: (1280., 720.).into(), // Optional: Set initial size
                    ..default()
                }),
                ..default()
            })
        )
        // --- Essential Third-Party Plugins ---
        .add_plugins(EguiPlugin {
            // turn on multi-pass for the primary context (matches the docs example)
            enable_multipass_for_primary_context: true,
        })
        // --- Add Application Plugins ---
        .add_plugins(SheetsPlugin)     // Handles sheet data loading/saving/logic
        .add_plugins(EditorUiPlugin)  // Provides the egui interface

        // --- Run ---
        .run();
}