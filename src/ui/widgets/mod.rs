// src/ui/widgets/mod.rs
// FINAL VERSION AFTER REFACTORING
use bevy::prelude::*; // Keep Bevy prelude if used within module, remove if not.

// Declare the new modules for the split linked column editor
pub(crate) mod linked_column_cache;
pub(crate) mod linked_column_handler;
pub(crate) mod linked_column_visualization;
pub(crate) mod option_widgets; // <-- ADDED option widgets module

// Re-export the main handler function to be used by common.rs
pub(crate) use linked_column_handler::handle_linked_column_edit;
pub(crate) use option_widgets::{ui_option_bool, ui_option_numerical}; // <-- ADDED re-export

// Remove or comment out the old module if it existed
// pub(crate) mod linked_column_editor;