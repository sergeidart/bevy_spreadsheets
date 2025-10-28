// src/sheets/systems/logic/update_column_validator/mod.rs
// Module for column validator update system

// Sub-modules organized by functionality
mod cell_population;
mod content_copy;
mod hierarchy;
mod structure_conversion;
mod update_column_validator_impl;

// Re-export the main handler function
pub use update_column_validator_impl::handle_update_column_validator;
