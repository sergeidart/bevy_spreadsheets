// src/sheets/systems/logic/mod.rs

// Declare modules for each handler
pub mod add_row;
pub mod delete_sheet;
pub mod rename_sheet;
pub mod update_cell;
pub mod update_column_name;
pub mod update_column_validator;

// Re-export the handler functions for easier use in plugin.rs
pub use add_row::handle_add_row_request;
pub use delete_sheet::handle_delete_request;
pub use rename_sheet::handle_rename_request;
pub use update_cell::handle_cell_update;
pub use update_column_name::handle_update_column_name;
pub use update_column_validator::handle_update_column_validator;