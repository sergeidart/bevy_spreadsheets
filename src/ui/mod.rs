// src/ui/mod.rs
use bevy::prelude::*;
use bevy_egui::EguiContextPass;

// Declare UI element modules
pub mod elements;
pub mod common;
pub mod validation;
pub mod systems;
pub mod widgets;

// Import the editor UI system from its new location
use elements::editor::generic_sheet_editor_ui;
// --- MODIFIED: Import EditorWindowState to initialize it ---
use elements::editor::state::EditorWindowState;
// --- END MODIFIED ---
// Import the new feedback handling system
use systems::handle_ui_feedback;
use systems::clear_ui_feedback_on_sheet_change;


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
            .init_resource::<UiFeedbackState>()
            // --- MODIFIED: Initialize EditorWindowState as a resource ---
            .init_resource::<EditorWindowState>()
            // --- END MODIFIED ---
            // Ensure we clear transient feedback on sheet changes before processing new feedback events
            .add_systems(Update, clear_ui_feedback_on_sheet_change)
            .add_systems(Update, handle_ui_feedback)
            .add_systems(EguiContextPass, generic_sheet_editor_ui);

        info!("EditorUiPlugin initialized with EditorWindowState as a resource.");
    }
}