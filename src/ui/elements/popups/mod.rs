// src/ui/elements/popups/mod.rs

// Declare the individual popup modules
pub mod ai_rule_popup;
pub mod column_options_popup;
pub mod delete_confirm_popup;
// NEW: Declare new_sheet_popup module
pub mod new_sheet_popup;
pub mod rename_popup;
pub mod settings_popup; 

// Declare the refactored modules for column options
mod column_options_ui;
mod column_options_validator;
mod column_options_on_close;

// Re-export the main popup functions for easier access
pub use ai_rule_popup::show_ai_rule_popup;
pub use column_options_popup::show_column_options_popup;
pub use delete_confirm_popup::show_delete_confirm_popup;
// NEW: Re-export new_sheet_popup function
pub use new_sheet_popup::show_new_sheet_popup;
pub use rename_popup::show_rename_popup;
pub use settings_popup::show_settings_popup;