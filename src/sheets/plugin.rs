// src/sheets/plugin.rs
use bevy::prelude::*;

use super::resources::{SheetRegistry, SheetRenderCache}; 
use super::events::{
    AddSheetRowRequest, AiTaskResult, AiBatchTaskResult, JsonSheetUploaded, RequestDeleteRows,
    RequestDeleteSheet, RequestDeleteSheetFile, RequestInitiateFileUpload,
    RequestProcessUpload, RequestRenameSheet, RequestRenameSheetFile,
    RequestUpdateColumnName, RequestUpdateColumnValidator,
    SheetOperationFeedback, UpdateCellEvent,
    SheetDataModifiedInRegistryEvent, RequestSheetRevalidation,
    RequestDeleteColumns, RequestAddColumn, RequestReorderColumn,
    // NEW: Import RequestCreateNewSheet
    RequestCreateNewSheet, RequestToggleAiRowGeneration,
    // Category events
    RequestCreateCategory, RequestDeleteCategory, RequestCreateCategoryDirectory, RequestRenameCategory, RequestRenameCategoryDirectory, RequestMoveSheetToCategory,
};
use super::systems; 
use crate::ui::systems::{handle_ai_task_results, handle_ai_batch_results}; 
use crate::ui::systems::apply_throttled_ai_changes; 
use crate::ui::systems::forward_events; 
use crate::ui::systems::apply_pending_structure_key_selection;
use super::systems::logic::handle_sheet_render_cache_update;
use super::systems::logic::sync_structure::{PendingStructureCascade, handle_emit_structure_cascade_events};


#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum SheetSystemSet {
    UserInput,
    ApplyChanges, 
    ProcessAsyncResults, 
    UpdateCaches, 
    FileOperations,
}

pub struct SheetsPlugin;

impl Plugin for SheetsPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                SheetSystemSet::UserInput,
                SheetSystemSet::ApplyChanges.after(SheetSystemSet::UserInput),
                SheetSystemSet::ProcessAsyncResults.after(SheetSystemSet::ApplyChanges),
                SheetSystemSet::UpdateCaches.after(SheetSystemSet::ProcessAsyncResults), 
                SheetSystemSet::FileOperations.after(SheetSystemSet::UpdateCaches),
            ),
        );

    app.init_resource::<SheetRegistry>();
        app.init_resource::<SheetRenderCache>();
    app.init_resource::<PendingStructureCascade>();

        app.add_event::<AddSheetRowRequest>()
            .add_event::<RequestAddColumn>()
            .add_event::<RequestReorderColumn>()
            // NEW: Register RequestCreateNewSheet event
            .add_event::<RequestCreateNewSheet>()
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
            .add_event::<RequestDeleteColumns>()
            .add_event::<AiTaskResult>()
            .add_event::<AiBatchTaskResult>()
            .add_event::<SheetDataModifiedInRegistryEvent>()
            .add_event::<RequestSheetRevalidation>()
            .add_event::<RequestToggleAiRowGeneration>()
            .add_event::<RequestMoveSheetToCategory>();
        // Category management events
        app.add_event::<RequestCreateCategory>()
            .add_event::<RequestDeleteCategory>()
            .add_event::<RequestCreateCategoryDirectory>()
            .add_event::<RequestRenameCategory>()
            .add_event::<RequestRenameCategoryDirectory>();

        app.add_systems(
            Startup,
            (
                systems::io::startup::register_default_sheets_if_needed,
                ApplyDeferred, 
                systems::io::startup::load_data_for_registered_sheets,
                ApplyDeferred, 
                systems::io::startup::scan_filesystem_for_unregistered_sheets,
                ApplyDeferred, 
                handle_sheet_render_cache_update, 
                ApplyDeferred, 
            )
                .chain(),
        );

        app.add_systems(
            Update,
            (systems::io::handle_initiate_file_upload,)
                .in_set(SheetSystemSet::UserInput),
        );

        app.add_systems(
            Update,
            (
                systems::io::handle_process_upload_request,
                ApplyDeferred, 
                systems::io::handle_json_sheet_upload, 

                systems::logic::handle_rename_request,
                systems::logic::handle_delete_request,
                systems::logic::handle_add_row_request,
                systems::logic::handle_toggle_ai_row_generation,
                systems::logic::handle_add_column_request,
                systems::logic::handle_reorder_column_request,
                // NEW: Add system for creating sheets
                systems::logic::handle_create_new_sheet_request,
                // Category create/delete
                systems::logic::handle_create_category_request,
                systems::logic::handle_delete_category_request,
                systems::logic::handle_rename_category_request,
                systems::logic::handle_delete_rows_request,
                systems::logic::handle_move_sheet_to_category_request,
                systems::logic::handle_delete_columns_request,
                systems::logic::handle_update_column_name,
                systems::logic::handle_update_column_validator,
                systems::logic::handle_cell_update, 
            )
                .chain() 
                .in_set(SheetSystemSet::ApplyChanges),
        );
        
        app.add_systems(
            Update,
            (
                forward_events::<AiTaskResult>,
                forward_events::<AiBatchTaskResult>,
                ApplyDeferred,
                apply_pending_structure_key_selection,
                handle_ai_task_results,
                handle_ai_batch_results,
                apply_throttled_ai_changes,
            )
            .chain() 
            .in_set(SheetSystemSet::ProcessAsyncResults)
        );

        app.add_systems(
            Update,
            (
                handle_sheet_render_cache_update,
                systems::logic::handle_sync_virtual_structure_sheet,
                handle_emit_structure_cascade_events,
            )
            .in_set(SheetSystemSet::UpdateCaches)
        );

        app.add_systems(
            Update,
            (
                systems::io::handle_delete_sheet_file_request,
                systems::io::handle_rename_sheet_file_request,
                systems::io::handle_create_category_directory_request,
                systems::io::handle_rename_category_directory_request,
            )
                .in_set(SheetSystemSet::FileOperations),
        );

        info!("SheetsPlugin initialized (with SheetRenderCache).");
    }
}