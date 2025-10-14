// src/sheets/systems/logic/update_column_validator/mod.rs
// Module for column validator update system

// Sub-modules organized by functionality
mod cell_population;
mod content_copy;
mod hierarchy;
mod structure_conversion;
mod update_column_validator_impl;

// Re-export public functions for backward compatibility
pub use cell_population::{
    ensure_structure_cells_not_empty, handle_structure_conversion_from, populate_structure_rows,
};
pub use content_copy::copy_parent_content_to_structure_table;
pub use hierarchy::{calculate_hierarchy_depth, create_structure_technical_columns};
pub use structure_conversion::handle_structure_conversion_to;

// Re-export the main handler function
pub use update_column_validator_impl::handle_update_column_validator;
