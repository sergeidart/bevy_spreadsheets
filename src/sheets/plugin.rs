// src/sheets/plugin.rs
use bevy::prelude::*;

// ADDED SheetRenderCache, REMOVED SheetValidationState
use super::resources::{SheetRegistry, SheetRenderCache}; //SheetValidationState
use super::events::{
    AddSheetRowRequest, AiTaskResult, JsonSheetUploaded, RequestDeleteRows,
    RequestDeleteSheet, RequestDeleteSheetFile, RequestInitiateFileUpload,
    RequestProcessUpload, RequestRenameSheet, RequestRenameSheetFile,
    RequestUpdateColumnName, RequestUpdateColumnValidator,
    SheetOperationFeedback, UpdateCellEvent, RequestUpdateColumnWidth,
    SheetDataModifiedInRegistryEvent, RequestSheetRevalidation,
};
use super::systems; // Keep access to sheets::systems
use crate::ui::systems::handle_ai_task_results;
use crate::ui::systems::forward_events;
// ADDED render cache update system, REMOVED validation system
// use super::systems::logic::handle_sheet_revalidation_request;
use super::systems::logic::handle_sheet_render_cache_update;


#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum SheetSystemSet {
    UserInput,
    ApplyChanges,
    // RENAMED set for combined validation and render cache update
    UpdateCaches, // Was Validation
    FileOperations,
}

pub struct SheetsPlugin;

impl Plugin for SheetsPlugin {
    fn build(&self, app: &mut App) {
        // Configure system sets for ordering
        app.configure_sets(
            Update,
            (
                SheetSystemSet::UserInput,
                SheetSystemSet::ApplyChanges.after(SheetSystemSet::UserInput),
                // RENAMED Validation set to UpdateCaches
                SheetSystemSet::UpdateCaches.after(SheetSystemSet::ApplyChanges),
                SheetSystemSet::FileOperations.after(SheetSystemSet::UpdateCaches),
            ),
        );

        // --- Resource Initialization ---
        app.init_resource::<SheetRegistry>();
        // ADDED Render Cache Resource, REMOVED Validation State Resource
        app.init_resource::<SheetRenderCache>();
        // app.init_resource::<SheetValidationState>(); // REMOVED

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
            .add_event::<AiTaskResult>()
            .add_event::<RequestUpdateColumnWidth>()
            .add_event::<SheetDataModifiedInRegistryEvent>()
            .add_event::<RequestSheetRevalidation>();

        // --- Startup Systems ---
        // Added apply_deferred after each step that might modify registry
        // and trigger cache update event. Added final cache update system trigger.
        app.add_systems(
            Startup,
            (
                systems::io::startup::register_default_sheets_if_needed,
                apply_deferred, // Apply default sheet registration
                systems::io::startup::load_data_for_registered_sheets,
                apply_deferred, // Apply loaded data & trigger events
                systems::io::startup::scan_filesystem_for_unregistered_sheets,
                apply_deferred, // Apply scanned data & trigger events
                // Run render cache update system once after all startup loading/scanning
                // This replaces handle_sheet_revalidation_request
                handle_sheet_render_cache_update,
                apply_deferred, // Apply initial render cache state
            )
                .chain(),
        );

        // --- Update Systems (Organized into Sets) ---
        app.add_systems(
            Update,
            (systems::io::handle_initiate_file_upload,)
                .in_set(SheetSystemSet::UserInput),
        );

        app.add_systems(
            Update,
            (
                // Input processing
                systems::io::handle_process_upload_request,
                apply_deferred,
                systems::io::handle_json_sheet_upload,
                apply_deferred,
                // Logic handlers (most now trigger events that update_render_cache will see)
                systems::logic::handle_rename_request,
                systems::logic::handle_delete_request,
                systems::logic::handle_add_row_request,
                systems::logic::handle_delete_rows_request,
                systems::logic::handle_update_column_name,
                systems::logic::handle_update_column_validator,
                systems::logic::handle_update_column_width,
                systems::logic::handle_cell_update, // This sends SheetDataModifiedInRegistryEvent
                apply_deferred,
                // AI results can also modify cells
                handle_ai_task_results, // This can lead to cell_update calls
                apply_deferred,
                forward_events::<AiTaskResult>.after(handle_ai_task_results),
            )
                .chain()
                .in_set(SheetSystemSet::ApplyChanges),
        );

        // ADDED: System to update render cache
        app.add_systems(
            Update,
            (
                // This system now handles validation and prepares data for rendering
                handle_sheet_render_cache_update,
                apply_deferred, // Apply render cache updates before file ops/UI
            )
            .in_set(SheetSystemSet::UpdateCaches) // Renamed from Validation
        );


        app.add_systems(
            Update,
            (
                systems::io::handle_delete_sheet_file_request,
                systems::io::handle_rename_sheet_file_request,
            )
                .in_set(SheetSystemSet::FileOperations),
        );

        info!("SheetsPlugin initialized (with SheetRenderCache).");
    }
}