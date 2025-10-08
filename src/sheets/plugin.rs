// src/sheets/plugin.rs
use bevy::prelude::*;

use super::events::{
    AddSheetRowRequest,
    AiBatchTaskResult,
    AiTaskResult,
    JsonSheetUploaded,
    RequestAddColumn,
    RequestCopyCell,
    RequestCreateAiSchemaGroup,
    // Category events
    RequestCreateCategory,
    // NEW: Import RequestCreateNewSheet
    RequestCreateNewSheet,
    RequestDeleteAiSchemaGroup,
    RequestDeleteCategory,
    RequestDeleteColumns,
    RequestDeleteRows,
    RequestDeleteSheet,
    RequestDeleteSheetFile,
    RequestInitiateFileUpload,
    RequestMoveSheetToCategory,
    RequestPasteCell,
    RequestProcessUpload,
    RequestRenameAiSchemaGroup,
    RequestRenameCategory,
    RequestRenameSheet,
    RequestRenameSheetFile,
    RequestReorderColumn,
    RequestSelectAiSchemaGroup,
    RequestSheetRevalidation,
    RequestToggleAiRowGeneration,
    RequestUpdateAiSendSchema,
    RequestUpdateAiStructureSend,
    RequestUpdateColumnName,
    RequestUpdateColumnValidator,
    SheetDataModifiedInRegistryEvent,
    SheetOperationFeedback,
    UpdateCellEvent,
    // Database migration events
    RequestMigrateJsonToDb,
    RequestUploadJsonToCurrentDb,
    MigrationCompleted,
    RequestExportSheetToJson,
};
use super::resources::{ClipboardBuffer, SheetRegistry, SheetRenderCache};
use super::systems;
use crate::sheets::database::systems::poll_migration_background;
use super::systems::logic::handle_sheet_render_cache_update;
use super::systems::logic::sync_structure::{
    handle_emit_structure_cascade_events, PendingStructureCascade,
};
use crate::sheets::systems::ai::results::{handle_ai_batch_results, handle_ai_task_results};
use crate::sheets::systems::ai::structure_processor::process_structure_ai_jobs;
use crate::sheets::systems::ai::throttled::apply_throttled_ai_changes;
use crate::ui::systems::apply_pending_structure_key_selection;
use crate::ui::systems::forward_events;

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
        app.init_resource::<ClipboardBuffer>();
    app.init_resource::<super::database::systems::MigrationBackgroundState>();

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
            .add_event::<RequestUpdateAiSendSchema>()
            .add_event::<RequestUpdateAiStructureSend>()
            .add_event::<RequestMoveSheetToCategory>()
            .add_event::<RequestCreateAiSchemaGroup>()
            .add_event::<RequestRenameAiSchemaGroup>()
            .add_event::<RequestDeleteAiSchemaGroup>()
            .add_event::<RequestSelectAiSchemaGroup>();
        // Category management events
        app.add_event::<RequestCreateCategory>()
            .add_event::<RequestDeleteCategory>()
            .add_event::<RequestRenameCategory>();
        // Clipboard events
        app.add_event::<RequestCopyCell>()
            .add_event::<RequestPasteCell>();
        
        // Database migration events
        app.add_event::<RequestMigrateJsonToDb>()
            .add_event::<RequestUploadJsonToCurrentDb>()
            .add_event::<MigrationCompleted>()
            .add_event::<crate::sheets::events::MigrationProgress>()
            .add_event::<RequestExportSheetToJson>();

        app.add_systems(
            Startup,
            (
                systems::io::startup::register_default_sheets_if_needed,
                ApplyDeferred,
                systems::io::startup::load_data_for_registered_sheets,
                ApplyDeferred,
                systems::io::startup::scan_filesystem_for_unregistered_sheets,
                ApplyDeferred,
                systems::io::startup::scan_and_load_database_files,
                ApplyDeferred,
                handle_sheet_render_cache_update,
                ApplyDeferred,
            )
                .chain(),
        );

        app.add_systems(
            Update,
            (systems::io::handle_initiate_file_upload,).in_set(SheetSystemSet::UserInput),
        );

        let apply_changes_stage_one = (
            systems::io::handle_process_upload_request,
            ApplyDeferred,
            systems::io::handle_json_sheet_upload,
            systems::logic::handle_rename_request,
            systems::logic::handle_delete_request,
            systems::logic::handle_add_row_request,
            systems::logic::handle_toggle_ai_row_generation,
            systems::logic::handle_update_ai_send_schema,
            systems::logic::handle_update_ai_structure_send,
            systems::logic::handle_create_ai_schema_group,
            systems::logic::handle_rename_ai_schema_group,
            systems::logic::handle_delete_ai_schema_group,
            systems::logic::handle_select_ai_schema_group,
        )
            .chain();

        let apply_changes_stage_two = (
            systems::logic::handle_add_column_request,
            systems::logic::handle_reorder_column_request,
            // NEW: Add system for creating sheets
            systems::logic::handle_create_new_sheet_request,
            // Category create/delete
            systems::logic::handle_create_category_request,
            systems::logic::handle_delete_category_request,
            systems::logic::handle_rename_category_request,
            systems::logic::handle_delete_rows_request,
        )
            .chain();

        let apply_changes_stage_three = (
            systems::logic::handle_move_sheet_to_category_request,
            systems::logic::handle_delete_columns_request,
            systems::logic::handle_update_column_name,
            systems::logic::handle_update_column_validator,
            systems::logic::handle_cell_update,
            // Clipboard operations
            systems::logic::handle_copy_cell,
            systems::logic::handle_paste_cell,
        )
            .chain();

        app.add_systems(
            Update,
            (
                apply_changes_stage_one,
                apply_changes_stage_two,
                apply_changes_stage_three,
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
                process_structure_ai_jobs,
                handle_ai_task_results,
                handle_ai_batch_results,
                apply_throttled_ai_changes,
            )
                .chain()
                .in_set(SheetSystemSet::ProcessAsyncResults),
        );

        app.add_systems(
            Update,
            (
                handle_sheet_render_cache_update,
                systems::logic::handle_sync_virtual_structure_sheet,
                handle_emit_structure_cascade_events,
                // Run inline structure migration once after sheets are loaded and caches are building
                systems::logic::run_inline_structure_migration_once,
                // UI progress updater for migration
                crate::ui::elements::popups::migration_popup::update_migration_progress_ui,
            )
                .in_set(SheetSystemSet::UpdateCaches),
        );

        app.add_systems(
            Update,
            (
                systems::io::handle_delete_sheet_file_request,
                systems::io::handle_rename_sheet_file_request,
                // Database migration systems
                super::database::handle_migration_requests,
                poll_migration_background,
                super::database::handle_upload_json_to_current_db,
                super::database::handle_export_requests,
                super::database::handle_migration_completion,
            )
                .in_set(SheetSystemSet::FileOperations),
        );

        info!("SheetsPlugin initialized (with SheetRenderCache).");
    }
}
