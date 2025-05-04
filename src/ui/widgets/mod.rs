// src/ui/widgets/mod.rs

// Declare the new modules for the split linked column editor
pub(super) mod linked_column_cache;
pub(super) mod linked_column_handler;
pub(super) mod linked_column_visualization;

// Re-export the main handler function to be used by common.rs
pub(crate) use linked_column_handler::handle_linked_column_edit;

// Remove or comment out the old module if it existed
// pub(crate) mod linked_column_editor;
