// src/ui/mod.rs
// FINAL VERSION AFTER REFACTORING
use bevy::prelude::*;
// Keep only necessary bevy_egui imports if EguiPlugin added elsewhere
use bevy_egui::EguiContextPass;

// Declare UI element modules
pub mod elements;
pub mod common;
pub mod validation; // <-- ADDED validation module
// Declare the new systems module
pub mod systems;
// Declare the new widgets module
pub mod widgets;

// Import the editor UI system from its new location
use elements::editor::generic_sheet_editor_ui; // Updated import path
// Import the new feedback handling system
use systems::handle_ui_feedback;

// --- Define the Feedback Resource ---
/// Resource to hold feedback messages for the UI.
#[derive(Resource, Default, Debug, Clone)]
pub struct UiFeedbackState {
    pub last_message: String,
    pub is_error: bool,
}


/// Plugin for the standalone spreadsheet editor UI.
pub struct EditorUiPlugin;

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        app
            // Initialize the UI feedback resource
            .init_resource::<UiFeedbackState>()
            // Add the UI feedback handler system to run in Update schedule
            .add_systems(Update, handle_ui_feedback)
            // Add the main editor UI rendering system using EguiContextPass
            .add_systems(EguiContextPass, generic_sheet_editor_ui); // System name remains the same

        info!("EditorUiPlugin initialized with feedback handling.");
    }
}