// src/ui/mod.rs
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiContextPass}; // Removed EguiPlugin import if unused elsewhere here

// Declare UI element modules
pub mod elements;
pub mod common; // Don't forget common

// Import the editor UI system
use elements::editor::generic_sheet_editor_ui;

/// Plugin for the standalone spreadsheet editor UI.
pub struct EditorUiPlugin;

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            // EguiPlugin is already added in main.rs, so remove it from here.
            // .add_plugins(EguiPlugin { enable_multipass_for_primary_context: true }) // <-- REMOVE THIS LINE
            // Run your UI system in the EguiContextPass schedule:
            .add_systems(EguiContextPass, generic_sheet_editor_ui); // Keep this

        info!("EditorUiPlugin initialized.");
    }
}