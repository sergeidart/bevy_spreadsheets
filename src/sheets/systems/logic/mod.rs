// src/sheets/systems/logic/mod.rs

pub mod add_row;
pub mod add_column;
pub mod reorder_column;
// NEW: Declare create_sheet module
pub mod create_sheet;
pub mod delete_sheet;
pub mod rename_sheet;
pub mod update_cell;
pub mod update_column_name;
pub mod update_column_validator;
pub mod delete_rows;
pub mod delete_columns;
pub mod update_column_width;
pub mod update_render_cache; 

pub use add_row::handle_add_row_request;
pub use add_column::handle_add_column_request;
pub use reorder_column::handle_reorder_column_request;
// NEW: Re-export create_sheet handler
pub use create_sheet::handle_create_new_sheet_request;
pub use delete_sheet::handle_delete_request;
pub use rename_sheet::handle_rename_request;
pub use update_cell::handle_cell_update;
pub use update_column_name::handle_update_column_name;
pub use update_column_validator::handle_update_column_validator;
pub use delete_rows::handle_delete_rows_request;
pub use delete_columns::handle_delete_columns_request;
pub use update_column_width::handle_update_column_width;
pub use update_render_cache::handle_sheet_render_cache_update;