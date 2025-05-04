// src/ui/elements/popups/mod.rs

// Declare the individual popup modules
pub mod column_options_popup;
pub mod delete_confirm_popup;
pub mod rename_popup;

// Re-export the popup functions for easier access
pub use column_options_popup::show_column_options_popup;
pub use delete_confirm_popup::show_delete_confirm_popup;
pub use rename_popup::show_rename_popup;