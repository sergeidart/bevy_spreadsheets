// src/ui/mod.rs
use bevy::prelude::*;
use bevy_egui::EguiContextPass;

// Declare UI element modules
pub mod common;
pub mod elements;
pub mod systems;
pub mod validation;
pub mod widgets;
pub mod elements_persist_support {}

// Import the editor UI system from its new location
use elements::editor::generic_sheet_editor_ui;
// --- MODIFIED: Import EditorWindowState to initialize it ---
use elements::editor::prefs::{load_prefs, save_prefs, UiPrefs};
use elements::editor::state::EditorWindowState;
// --- END MODIFIED ---
// Import the new feedback handling system
use systems::clear_ui_feedback_on_sheet_change;
use systems::handle_ui_feedback;
// Import child table loader system
use crate::sheets::systems::ai_review::child_table_loader::load_structure_child_tables_system;

#[derive(Resource, Default, Debug, Clone)]
pub struct UiFeedbackState {
    pub last_message: String,
    pub is_error: bool,
}

/// Plugin for the standalone spreadsheet editor UI.
pub struct EditorUiPlugin;

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiFeedbackState>()
            // --- MODIFIED: Initialize EditorWindowState as a resource ---
            .init_resource::<EditorWindowState>()
            .init_resource::<elements::popups::MigrationPopupState>()
            // Load UI prefs on startup
            .add_systems(Startup, load_ui_prefs_startup)
            // --- END MODIFIED ---
            // Load structure child tables once when AI Review starts (before UI)
            .add_systems(Update, load_structure_child_tables_system)
            // Ensure we clear transient feedback on sheet changes before processing new feedback events
            .add_systems(Update, clear_ui_feedback_on_sheet_change)
            // Persist UI prefs when toggled
            .add_systems(Update, persist_ui_prefs_if_changed)
            .add_systems(Update, handle_ui_feedback)
            .add_systems(EguiContextPass, generic_sheet_editor_ui);

        info!("EditorUiPlugin initialized with EditorWindowState as a resource.");
    }
}

fn load_ui_prefs_startup(mut state: ResMut<EditorWindowState>) {
    let prefs = load_prefs();
    state.category_picker_expanded = prefs.category_picker_expanded;
    state.sheet_picker_expanded = prefs.sheet_picker_expanded;
    state.ai_groups_expanded = prefs.ai_groups_expanded;
}

fn persist_ui_prefs_if_changed(state: Res<EditorWindowState>, mut last: Local<Option<UiPrefs>>) {
    // Initialize on first run
    if last.is_none() {
        *last = Some(UiPrefs {
            category_picker_expanded: state.category_picker_expanded,
            sheet_picker_expanded: state.sheet_picker_expanded,
            ai_groups_expanded: state.ai_groups_expanded,
        });
        return;
    }
    let cur = UiPrefs {
        category_picker_expanded: state.category_picker_expanded,
        sheet_picker_expanded: state.sheet_picker_expanded,
        ai_groups_expanded: state.ai_groups_expanded,
    };
    if last.as_ref().map(|p| p != &cur).unwrap_or(true) {
        save_prefs(&cur);
        *last = Some(cur);
    }
}
