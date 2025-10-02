// src/ui/elements/editor/mod.rs

// Declare the submodules for the editor components
pub mod main_editor;
pub mod state;
pub mod structure_navigation;
pub mod table_body;
pub mod table_header; // This is now the orchestrator

// NEW MODULES
pub mod editor_ai_log;
pub mod editor_event_handling;
pub mod editor_mode_panels;
pub mod editor_popups_integration;
pub mod editor_sheet_display;
pub mod prefs;

// Existing AI modules
// AI-related modules moved to ui::elements::ai_review

// Re-export the main UI function and potentially the state if needed elsewhere
pub use main_editor::generic_sheet_editor_ui;
pub use state::EditorWindowState;
