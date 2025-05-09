// src/sheets/plugin.rs
use bevy::prelude::*;

use super::resources::{SheetRegistry, SheetRenderCache}; 
use super::events::{
    AddSheetRowRequest, AiTaskResult, JsonSheetUploaded, RequestDeleteRows,
    RequestDeleteSheet, RequestDeleteSheetFile, RequestInitiateFileUpload,
    RequestProcessUpload, RequestRenameSheet, RequestRenameSheetFile,
    RequestUpdateColumnName, RequestUpdateColumnValidator,
    SheetOperationFeedback, UpdateCellEvent, RequestUpdateColumnWidth,
    SheetDataModifiedInRegistryEvent, RequestSheetRevalidation,
    RequestDeleteColumns, RequestAddColumn, RequestReorderColumn,
    // NEW: Import RequestCreateNewSheet
    RequestCreateNewSheet,
};
use super::systems; 
use crate::ui::systems::handle_ai_task_results; 
use crate::ui::systems::forward_events; 
use super::systems::logic::handle_sheet_render_cache_update;


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
            .add_event::<RequestUpdateColumnWidth>()
            .add_event::<SheetDataModifiedInRegistryEvent>()
            .add_event::<RequestSheetRevalidation>();

        app.add_systems(
            Startup,
            (
                systems::io::startup::register_default_sheets_if_needed,
                apply_deferred, 
                systems::io::startup::load_data_for_registered_sheets,
                apply_deferred, 
                systems::io::startup::scan_filesystem_for_unregistered_sheets,
                apply_deferred, 
                handle_sheet_render_cache_update, 
                apply_deferred, 
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
                apply_deferred, 
                systems::io::handle_json_sheet_upload, 

                systems::logic::handle_rename_request,
                systems::logic::handle_delete_request,
                systems::logic::handle_add_row_request,
                systems::logic::handle_add_column_request,
                systems::logic::handle_reorder_column_request,
                // NEW: Add system for creating sheets
                systems::logic::handle_create_new_sheet_request,
                systems::logic::handle_delete_rows_request,
                systems::logic::handle_delete_columns_request,
                systems::logic::handle_update_column_name,
                systems::logic::handle_update_column_validator,
                systems::logic::handle_update_column_width,
                systems::logic::handle_cell_update, 
            )
                .chain() 
                .in_set(SheetSystemSet::ApplyChanges),
        );
        
        app.add_systems(
            Update,
            (
                forward_events::<AiTaskResult>, 
                apply_deferred, 
                handle_ai_task_results, 
            )
            .chain() 
            .in_set(SheetSystemSet::ProcessAsyncResults)
        );

        app.add_systems(
            Update,
            (
                handle_sheet_render_cache_update, 
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