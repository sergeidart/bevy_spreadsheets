// src/sheets/systems/logic/update_column_validator/mod.rs
// Module for column validator update system

// Sub-modules organized by functionality
mod cell_population;
mod content_copy;
mod db_operations;
mod hierarchy;
mod persistence;
mod structure_conversion;
mod structure_recreation_handler;
mod update_column_validator_impl;
mod validation;

// Re-export the main handler function
pub use update_column_validator_impl::handle_update_column_validator;
pub use structure_recreation_handler::handle_structure_table_recreation;
