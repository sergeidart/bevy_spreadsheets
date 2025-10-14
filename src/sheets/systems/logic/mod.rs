// src/sheets/systems/logic/mod.rs

pub mod add_column;
pub mod add_row;
pub mod reorder_column;
pub mod categories;
pub mod cell_background_logic;
pub mod cell_validator_logic;
pub mod cell_widget_helpers;
pub mod clipboard;
pub mod create_sheet;
pub mod delete_columns;
pub mod delete_rows;
pub mod delete_sheet;
pub mod migrate_inline_structures;
pub mod move_sheet;
pub mod rename_sheet;
pub mod structure_preview_logic;
pub mod sync_structure;
pub mod update_cell;
pub mod update_column_name;
pub mod update_column_validator;
pub mod update_render_cache;

pub use add_column::handle_add_column_request;
pub use add_row::handle_add_row_request;
pub use add_row::handle_add_rows_batch_request;
pub use add_row::handle_create_ai_schema_group;
pub use add_row::handle_delete_ai_schema_group;
pub use add_row::handle_rename_ai_schema_group;
pub use add_row::handle_select_ai_schema_group;
pub use add_row::handle_toggle_ai_row_generation;
pub use add_row::handle_update_ai_send_schema;
pub use add_row::handle_update_ai_structure_send;
pub use add_row::handle_update_column_ai_include;
pub use reorder_column::handle_reorder_column_request;
pub use categories::{
    handle_create_category_request, handle_delete_category_request, handle_rename_category_request,
};
pub use cell_background_logic::determine_cell_background_color;
pub use cell_validator_logic::{
    determine_effective_validation_state, is_column_ai_included,
    is_structure_column_ai_included, prefetch_linked_column_values, LinkedColumnPrefetch,
};
pub use cell_widget_helpers::{compute_structure_root_and_path, resolve_structure_override_for_menu};
pub use clipboard::{handle_copy_cell, handle_paste_cell};
pub use create_sheet::handle_create_new_sheet_request;
pub use delete_columns::handle_delete_columns_request;
pub use delete_rows::handle_delete_rows_request;
pub use delete_sheet::handle_delete_request;
pub use migrate_inline_structures::migrate_inline_structure_data;
pub use migrate_inline_structures::run_inline_structure_migration_once;
pub use move_sheet::handle_move_sheet_to_category_request;
pub use rename_sheet::handle_rename_request;
pub use structure_preview_logic::{generate_structure_preview, generate_structure_preview_from_rows, generate_structure_preview_from_rows_with_headers};
pub use sync_structure::handle_sync_virtual_structure_sheet;
pub use update_cell::handle_cell_update;
pub use update_column_name::handle_update_column_name;
pub use update_column_validator::handle_update_column_validator;
pub use update_render_cache::handle_sheet_render_cache_update;
