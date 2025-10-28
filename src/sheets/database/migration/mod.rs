// src/sheets/database/migration/mod.rs

pub mod dependency_handler;
pub mod io_helpers;
pub mod json_extractor;
pub mod json_migration;
pub mod occasional_fixes;
pub mod fix_row_index_duplicates;
pub mod parent_key_to_row_index;
pub mod cleanup_temp_new_row_index;
pub mod hide_temp_new_row_index_in_metadata;
pub mod remove_grand_parent_columns;

// Re-export main types and functions for backward compatibility
pub use dependency_handler::DependencyHandler;
pub use io_helpers::{IoHelpers, JsonSheetPair};
pub use json_migration::{JsonMigration, MigrationReport};
pub use occasional_fixes::OccasionalFixManager;

use rusqlite::Connection;
use std::path::Path;

use super::error::DbResult;

/// Main migration tools struct (for backward compatibility)
pub struct MigrationTools;

impl MigrationTools {
    /// Migrate a single sheet from JSON files to database
    pub fn migrate_sheet_from_json(
        conn: &mut Connection,
        json_data_path: &Path,
        json_meta_path: &Path,
        table_name: &str,
        display_order: Option<i32>,
        on_rows_chunk: Option<&mut dyn FnMut(usize)>,
    ) -> DbResult<()> {
        JsonMigration::migrate_sheet_from_json(
            conn,
            json_data_path,
            json_meta_path,
            table_name,
            display_order,
            on_rows_chunk,
        )
    }

    /// Find all linked sheets referenced in metadata
    pub fn find_linked_sheets(metadata: &crate::sheets::definitions::SheetMetadata) -> Vec<String> {
        DependencyHandler::find_linked_sheets(metadata)
    }

    /// Scan folder for JSON pairs and their dependencies
    pub fn scan_json_folder(
        folder_path: &Path,
    ) -> DbResult<std::collections::HashMap<String, JsonSheetPair>> {
        IoHelpers::scan_json_folder(folder_path)
    }

    /// Migrate multiple sheets with dependency resolution
    pub fn migrate_folder_to_db(
        db_path: &Path,
        folder_path: &Path,
        create_new_db: bool,
    ) -> DbResult<MigrationReport> {
        JsonMigration::migrate_folder_to_db(db_path, folder_path, create_new_db)
    }

    /// Order sheets so dependencies are migrated first
    pub fn order_sheets_by_dependency(
        sheets: &std::collections::HashMap<String, JsonSheetPair>,
    ) -> Vec<String> {
        DependencyHandler::order_sheets_by_dependency(sheets)
    }

    /// Export sheet from database to JSON
    pub fn export_sheet_to_json(
        conn: &Connection,
        table_name: &str,
        output_folder: &Path,
    ) -> DbResult<()> {
        IoHelpers::export_sheet_to_json(conn, table_name, output_folder)
    }
}
