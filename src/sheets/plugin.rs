// src/sheets/plugin.rs
use bevy::prelude::*;

use super::resources::SheetRegistry;
use super::events::{
    // Import existing events
    AddSheetRowRequest, AiTaskResult, JsonSheetUploaded, RequestDeleteRows,
    RequestDeleteSheet, RequestDeleteSheetFile, RequestInitiateFileUpload,
    RequestProcessUpload, RequestRenameSheet, RequestRenameSheetFile,
    RequestUpdateColumnName, RequestUpdateColumnValidator,
    SheetOperationFeedback, UpdateCellEvent, RequestUpdateColumnWidth,
    SheetDataModifiedInRegistryEvent, // <-- ADDED IMPORT
};
use super::systems; // Keep access to sheets::systems
// Import the AI result handler from correct location
use crate::ui::systems::handle_ai_task_results;
// Import the event forwarding system helper from correct location
use crate::ui::systems::forward_events;

// Define system sets for ordering
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum SheetSystemSet {
    UserInput,      // Systems reacting directly to UI events
    ApplyChanges,   // Systems processing data and modifying registry
    FileOperations, // Systems performing file IO requests
}

/// Plugin for managing sheet data within the standalone app.
pub struct SheetsPlugin;

impl Plugin for SheetsPlugin {
    fn build(&self, app: &mut App) {
        // Configure system sets for ordering
        app.configure_sets(
            Update,
            (
                SheetSystemSet::UserInput,
                SheetSystemSet::ApplyChanges.after(SheetSystemSet::UserInput),
                SheetSystemSet::FileOperations.after(SheetSystemSet::ApplyChanges),
            ),
        );

        // --- Resource Initialization ---
        app.init_resource::<SheetRegistry>();

        // --- Event Registration ---
        app.add_event::<AddSheetRowRequest>()
            .add_event::<JsonSheetUploaded>()
            .add_event::<RequestRenameSheet>()
            .add_event::<RequestDeleteSheet>()
            .add_event::<RequestDeleteSheetFile>()
            .add_event::<RequestRenameSheetFile>()
            .add_event::<SheetOperationFeedback>()
            .add_event::<RequestInitiateFileUpload>()
            .add_event::<RequestProcessUpload>()
            .add_event::<RequestUpdateColumnName>()
            .add_event::<RequestUpdateColumnValidator>()
            .add_event::<UpdateCellEvent>()
            .add_event::<RequestDeleteRows>()
            .add_event::<AiTaskResult>() // Register AI Result Event
            .add_event::<RequestUpdateColumnWidth>()
            .add_event::<SheetDataModifiedInRegistryEvent>(); // <-- REGISTER EVENT

        // --- Startup Systems ---
        // Updated paths to reflect the new startup module structure
        app.add_systems(
            Startup,
            (
                // 1. Register default sheets if data dir empty/missing
                systems::io::startup::register_default_sheets_if_needed,
                apply_deferred, // Ensure registry changes are applied
                // 2. Load data for any sheets already in the registry
                systems::io::startup::load_data_for_registered_sheets,
                apply_deferred, // Ensure loaded data is available
                // 3. Scan filesystem for any remaining unregistered sheets
                systems::io::startup::scan_filesystem_for_unregistered_sheets,
            )
                .chain(), // Run startup systems sequentially
        );

        // --- Update Systems (Organized into Sets) ---
        // (Update systems remain largely the same, paths don't change here)
        app.add_systems(
            Update,
            (systems::io::handle_initiate_file_upload,)
                .in_set(SheetSystemSet::UserInput),
        );
        app.add_systems(
            Update,
            (
                systems::io::handle_process_upload_request,
                apply_deferred,
                systems::io::handle_json_sheet_upload,
                apply_deferred,
                systems::logic::handle_rename_request,
                systems::logic::handle_delete_request,
                systems::logic::handle_add_row_request,
                systems::logic::handle_delete_rows_request,
                systems::logic::handle_update_column_name,
                systems::logic::handle_update_column_validator,
                systems::logic::handle_update_column_width, 
                systems::logic::handle_cell_update,
                apply_deferred, // Apply registry changes before AI results/saving
                handle_ai_task_results,
                apply_deferred,
                forward_events::<AiTaskResult>.after(handle_ai_task_results),
            )
                .chain()
                .in_set(SheetSystemSet::ApplyChanges),
        );
        app.add_systems(
            Update,
            (
                systems::io::handle_delete_sheet_file_request,
                systems::io::handle_rename_sheet_file_request,
            )
                .in_set(SheetSystemSet::FileOperations),
        );

        info!("SheetsPlugin initialized (with Startup Split and Width Handler)."); // Updated log
    }
}