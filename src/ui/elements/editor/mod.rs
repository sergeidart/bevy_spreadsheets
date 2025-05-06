// src/ui/elements/editor/mod.rs

// Declare the submodules for the editor components
pub mod state;
pub mod table_body;
pub mod table_header;
pub mod main_editor; // The main assembly function
// pub mod ai_features; // <-- REMOVED old module

// Declare new AI modules
pub mod ai_control_panel;
pub mod ai_review_ui;
pub mod ai_helpers;


// Re-export the main UI function and potentially the state if needed elsewhere
pub use main_editor::generic_sheet_editor_ui;
pub use state::EditorWindowState; // Re-export state struct
// Do not re-export AI functions unless explicitly needed elsewhere