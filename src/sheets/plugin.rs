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
use crate::ui::systems::handle_ai_task_results; // Assuming this is where it's defined
use crate::ui::systems::forward_events; // Assuming this is where it's defined
// ADDED render cache update system, REMOVED validation system
// use super::systems::logic::handle_sheet_revalidation_request;
use super::systems::logic::handle_sheet_render_cache_update;


#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum SheetSystemSet {
    UserInput,
    ApplyChanges, // For systems that directly modify registry or send primary action events
    ProcessAsyncResults, // New set for forwarding and handling async results
    UpdateCaches, 
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
                // New set for async results processing
                SheetSystemSet::ProcessAsyncResults.after(SheetSystemSet::ApplyChanges),
                SheetSystemSet::UpdateCaches.after(SheetSystemSet::ProcessAsyncResults), // Update caches after results are processed
                SheetSystemSet::FileOperations.after(SheetSystemSet::UpdateCaches),
            ),
        );

        // --- Resource Initialization ---
        app.init_resource::<SheetRegistry>();
        app.init_resource::<SheetRenderCache>();

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
            .add_event::<AiTaskResult>() // Make sure it's registered
            .add_event::<RequestUpdateColumnWidth>()
            .add_event::<SheetDataModifiedInRegistryEvent>()
            .add_event::<RequestSheetRevalidation>();

        // --- Startup Systems ---
        app.add_systems(
            Startup,
            (
                systems::io::startup::register_default_sheets_if_needed,
                apply_deferred, 
                systems::io::startup::load_data_for_registered_sheets,
                apply_deferred, 
                systems::io::startup::scan_filesystem_for_unregistered_sheets,
                apply_deferred, 
                handle_sheet_render_cache_update, // Initial cache build
                apply_deferred, 
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
                systems::io::handle_process_upload_request,
                apply_deferred, // Apply results of upload processing
                systems::io::handle_json_sheet_upload, // This will modify registry
                // apply_deferred, // Already handled by virtue of ProcessAsyncResults running after

                systems::logic::handle_rename_request,
                systems::logic::handle_delete_request,
                systems::logic::handle_add_row_request,
                systems::logic::handle_delete_rows_request,
                systems::logic::handle_update_column_name,
                systems::logic::handle_update_column_validator,
                systems::logic::handle_update_column_width,
                systems::logic::handle_cell_update, // This sends SheetDataModifiedInRegistryEvent
                // apply_deferred should ensure these changes are seen by ProcessAsyncResults if needed,
                // but direct AI task results are handled separately.
            )
                .chain() // apply_deferred within this chain might be excessive if each system sends events handled later
                .in_set(SheetSystemSet::ApplyChanges),
        );
        
        // Systems for processing results from async tasks (like AI)
        app.add_systems(
            Update,
            (
                // Forward events first
                forward_events::<AiTaskResult>, // Reads SendEvent<AiTaskResult> and sends AiTaskResult
                apply_deferred, // Ensure AiTaskResult events are flushed
                // Then handle the actual AiTaskResult events
                handle_ai_task_results, // Reads AiTaskResult events
                // apply_deferred, // Results of this (e.g., UI state changes) will be applied before UpdateCaches
            )
            .chain() // Ensure sequential execution within this set
            .in_set(SheetSystemSet::ProcessAsyncResults)
        );


        app.add_systems(
            Update,
            (
                handle_sheet_render_cache_update, // Reads events like SheetDataModifiedInRegistryEvent
                // apply_deferred, // Apply render cache updates before file ops/UI
            )
            .in_set(SheetSystemSet::UpdateCaches) 
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