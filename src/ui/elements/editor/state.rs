// src/ui/elements/editor/state.rs
use bevy::prelude::Resource; // Import Resource derive

// Define the local state for the editor window
// Make it a Resource if we want Bevy to manage it globally,
// or keep using Local<EditorWindowState> in the system if preferred.
// For now, keep as a struct used with Local<T>.
#[derive(Default)]
pub struct EditorWindowState {
    pub selected_sheet_name: Option<String>,
    pub show_rename_popup: bool,
    pub rename_target: String,
    pub new_name_input: String,
    pub show_delete_confirm_popup: bool,
    pub delete_target: String,
    // Column Options Popup State
    pub show_column_options_popup: bool,
    pub options_column_target_sheet: String,
    pub options_column_target_index: usize,
    pub options_column_rename_input: String,
    pub options_column_filter_input: String,
    // Flag to indicate popup needs init when opened
    pub column_options_popup_needs_init: bool,
    // Flag to trigger save after popup or cell edits
    pub sheet_needs_save: bool,
    pub sheet_to_save: String,
}