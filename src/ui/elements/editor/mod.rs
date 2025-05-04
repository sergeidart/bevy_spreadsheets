// src/ui/elements/editor/mod.rs

// Declare the submodules for the editor components
pub mod state;
pub mod table_body;
pub mod table_header;
pub mod main_editor; // The main assembly function

// Re-export the main UI function and potentially the state if needed elsewhere
pub use main_editor::generic_sheet_editor_ui;
pub use state::EditorWindowState; // Re-export state struct