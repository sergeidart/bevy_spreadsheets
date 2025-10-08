mod validator_confirm_popup;
pub use validator_confirm_popup::show_validator_confirm_popup;
// src/ui/elements/popups/mod.rs

// Declare the individual popup modules
pub mod column_options_popup;
pub mod delete_confirm_popup;
// NEW: Declare new_sheet_popup module
pub mod ai_prompt_popup;
pub mod ai_rule_popup;
pub mod category_popups;
pub mod new_sheet_popup;
pub mod rename_popup;
pub mod settings_popup;
pub mod migration_popup;
pub mod add_table_popup;

// Declare the refactored modules for column options
mod column_options_on_close;
mod column_options_ui;
mod column_options_validator;
mod random_picker_popup;
mod random_picker_ui;

// Re-export the main popup functions for easier access
pub use column_options_popup::show_column_options_popup;
pub use delete_confirm_popup::show_delete_confirm_popup;
// NEW: Re-export new_sheet_popup function
pub use ai_rule_popup::show_ai_rule_popup;
pub use new_sheet_popup::show_new_sheet_popup;
pub use rename_popup::show_rename_popup;
pub use settings_popup::show_settings_popup;
pub use migration_popup::{show_migration_popup, MigrationPopupState};
pub use add_table_popup::show_add_table_popup;
// Note: show_ai_prompt_popup is invoked from AI control panel directly
pub use category_popups::{show_delete_category_confirm_popups, show_new_category_popup};
pub use random_picker_popup::show_random_picker_popup;
