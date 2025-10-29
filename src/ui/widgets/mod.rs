// src/ui/widgets/mod.rs

// Declare the new modules for the split linked column editor
pub(crate) mod linked_column_cache;
pub(crate) mod linked_column_handler;
pub(crate) mod linked_column_visualization;
pub(crate) mod option_widgets;
pub(crate) mod context_menu_helpers;
pub(crate) mod technical_column_widget;
pub(crate) mod structure_column_widget;

// Re-export the main handler function to be used by common.rs
pub(crate) use linked_column_handler::handle_linked_column_edit;

// Re-export context menu helper
pub(crate) use context_menu_helpers::add_cell_context_menu;

// Re-export option widget helpers
pub(crate) use option_widgets::{add_centered_checkbox, add_numeric_drag_value};

// Re-export technical column widget helpers
pub(crate) use technical_column_widget::render_technical_column;

// Re-export structure column widget
pub(crate) use structure_column_widget::render_structure_column;
