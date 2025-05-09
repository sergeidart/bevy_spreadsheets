// src/ui/elements/editor/mod.rs

// Declare the submodules for the editor components
pub mod state;
pub mod table_body;
pub mod table_header;
pub mod main_editor; // This is now the orchestrator

// NEW MODULES
pub mod editor_event_handling;
pub mod editor_popups_integration;
pub mod editor_mode_panels;
pub mod editor_sheet_display;
pub mod editor_ai_log;

// Existing AI modules
pub mod ai_control_panel;
pub mod ai_review_ui;
pub mod ai_helpers;
pub mod ai_panel_structs;


// Re-export the main UI function and potentially the state if needed elsewhere
pub use main_editor::generic_sheet_editor_ui;
pub use state::EditorWindowState;