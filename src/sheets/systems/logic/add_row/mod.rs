// src/sheets/systems/logic/add_row/mod.rs
// Module organization for add_row handlers

mod ai_config_handlers;
mod ai_schema_handlers;
mod cache_handlers;
mod common;
mod db_persistence;
mod json_persistence;
mod row_addition;

// Re-export public handlers
pub use ai_config_handlers::{
    handle_create_ai_schema_group, handle_delete_ai_schema_group, handle_rename_ai_schema_group,
    handle_select_ai_schema_group,
};
pub use ai_schema_handlers::{
    handle_toggle_ai_row_generation, handle_update_ai_send_schema,
    handle_update_ai_structure_send, handle_update_column_ai_include,
};
pub use row_addition::handle_add_row_request;
