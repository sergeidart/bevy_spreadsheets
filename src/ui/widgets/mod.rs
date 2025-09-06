// src/ui/widgets/mod.rs

// Declare the new modules for the split linked column editor
pub(crate) mod linked_column_cache;
pub(crate) mod linked_column_handler;
pub(crate) mod linked_column_visualization;
// Option widgets removed along with Option<T> column types

// Re-export the main handler function to be used by common.rs
pub(crate) use linked_column_handler::handle_linked_column_edit;