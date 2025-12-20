// src/ui/elements/editor/main_editor.rs
use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::window::Window;
use bevy_egui::{egui, EguiContexts};
use bevy_tokio_tasks::TokioTasksRuntime;

use super::editor_ai_log;
use super::editor_event_handling;
use super::editor_popups_integration;
use super::editor_sheet_display;
use super::state::{AiModeState, EditorWindowState, SheetInteractionState};
use crate::sheets::{
    database::daemon_resource::SharedDaemonClient,
    events::{
        AddSheetRowRequest, RequestAddColumn,
        RequestBatchUpdateColumnAiInclude, RequestCopyCell, RequestCreateAiSchemaGroup,
        RequestCreateCategory, RequestCreateNewSheet, RequestDeleteAiSchemaGroup,
        RequestDeleteCategory, RequestDeleteColumns, RequestDeleteRows, RequestDeleteSheet,
        RequestPasteCell, RequestRenameAiSchemaGroup, RequestRenameSheet, RequestReorderColumn,
        RequestSelectAiSchemaGroup, RequestSheetRevalidation, RequestToggleAiRowGeneration,
        RequestUpdateAiSendSchema, RequestUpdateAiStructureSend, RequestUpdateColumnAiInclude,
        RequestUpdateColumnName, RequestUpdateColumnValidator, SheetDataModifiedInRegistryEvent,
        UpdateCellEvent,
    },
    resources::{ClipboardBuffer, SheetRegistry, SheetRenderCache},
};
use crate::ui::{elements::top_panel::show_top_panel_orchestrator, UiFeedbackState};

use crate::visual_copier::{
    events::{
        PickFolderRequest, QueueTopPanelCopyEvent, RequestAppExit, ReverseTopPanelFoldersEvent,
        VisualCopierStateChanged,
    },
    resources::VisualCopierManager,
};
use crate::ApiKeyDisplayStatus;
use crate::SessionApiKey;

#[derive(SystemParam)]
pub struct SheetEventWriters<'w> {
    pub add_row: EventWriter<'w, AddSheetRowRequest>,
    pub add_column: EventWriter<'w, RequestAddColumn>,
    pub create_sheet: EventWriter<'w, RequestCreateNewSheet>,
    pub rename_sheet: EventWriter<'w, RequestRenameSheet>,
    pub rename_category: EventWriter<'w, crate::sheets::events::RequestRenameCategory>,
    pub delete_sheet: EventWriter<'w, RequestDeleteSheet>,
    pub upload_json_to_db: EventWriter<'w, crate::sheets::events::RequestUploadJsonToCurrentDb>,
    pub column_rename: EventWriter<'w, RequestUpdateColumnName>,
    pub column_validator: EventWriter<'w, RequestUpdateColumnValidator>,
    pub cell_update: EventWriter<'w, UpdateCellEvent>,
    pub delete_rows: EventWriter<'w, RequestDeleteRows>,
    pub delete_columns: EventWriter<'w, RequestDeleteColumns>,
    pub reorder_column: EventWriter<'w, RequestReorderColumn>,
    pub revalidate: EventWriter<'w, RequestSheetRevalidation>,
    pub toggle_ai_row_generation: EventWriter<'w, RequestToggleAiRowGeneration>,
    pub update_column_ai_include: EventWriter<'w, RequestUpdateColumnAiInclude>,
    pub batch_update_column_ai_include: EventWriter<'w, RequestBatchUpdateColumnAiInclude>,
    pub update_ai_send_schema: EventWriter<'w, RequestUpdateAiSendSchema>,
    pub update_ai_structure_send: EventWriter<'w, RequestUpdateAiStructureSend>,
    pub create_ai_schema_group: EventWriter<'w, RequestCreateAiSchemaGroup>,
    pub rename_ai_schema_group: EventWriter<'w, RequestRenameAiSchemaGroup>,
    pub select_ai_schema_group: EventWriter<'w, RequestSelectAiSchemaGroup>,
    pub delete_ai_schema_group: EventWriter<'w, RequestDeleteAiSchemaGroup>,
    // Category management
    pub create_category: EventWriter<'w, RequestCreateCategory>,
    pub delete_category: EventWriter<'w, RequestDeleteCategory>,
    pub move_sheet_to_category: EventWriter<'w, crate::sheets::events::RequestMoveSheetToCategory>,
    // Clipboard
    pub copy_cell: EventWriter<'w, RequestCopyCell>,
    pub paste_cell: EventWriter<'w, RequestPasteCell>,
    // Database migration
    pub migrate_json_to_db: EventWriter<'w, crate::sheets::events::RequestMigrateJsonToDb>,
    pub feedback: EventWriter<'w, crate::sheets::events::SheetOperationFeedback>,
    // Structure table recreation
    pub structure_recreation: EventWriter<'w, crate::sheets::events::RequestStructureTableRecreation>,
}

// Quick Copy controls moved into Settings popup; no dedicated top-row event writers required here.
#[derive(SystemParam)]
pub struct CopierEventWriters<'w> {
    pub pick_folder: EventWriter<'w, PickFolderRequest>,
    pub queue_top_panel_copy: EventWriter<'w, QueueTopPanelCopyEvent>,
    pub reverse_folders: EventWriter<'w, ReverseTopPanelFoldersEvent>,
    pub state_changed: EventWriter<'w, VisualCopierStateChanged>,
}

#[derive(SystemParam)]
pub struct EditorMiscParams<'w> {
    pub registry: ResMut<'w, SheetRegistry>,
    pub render_cache_res: Res<'w, SheetRenderCache>,
    pub clipboard_buffer: Res<'w, ClipboardBuffer>,
    pub ui_feedback: Res<'w, UiFeedbackState>,
    pub runtime: Res<'w, TokioTasksRuntime>,
    pub api_key_status_res: ResMut<'w, ApiKeyDisplayStatus>,
    pub session_api_key_res: ResMut<'w, SessionApiKey>,
    pub copier_manager: ResMut<'w, VisualCopierManager>,
    pub request_app_exit_writer: EventWriter<'w, RequestAppExit>,
    pub daemon_client: Res<'w, SharedDaemonClient>,
    pub director_session: ResMut<'w, crate::sheets::systems::ai::processor::DirectorSession>,
}

#[allow(clippy::too_many_arguments)]
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorWindowState>,
    mut migration_state: ResMut<crate::ui::elements::popups::MigrationPopupState>,
    mut sheet_writers: SheetEventWriters,
    mut copier_writers: CopierEventWriters,
    mut misc: EditorMiscParams,
    mut commands: Commands,
    mut sheet_data_modified_events: EventReader<SheetDataModifiedInRegistryEvent>,
    window_query: Query<Entity, With<Window>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    // Guard: If all windows are closed (app shutting down) skip egui usage to avoid panic
    if window_query.is_empty() {
        return;
    }
    let ctx = contexts.ctx_mut();
    let initial_selected_category = state.selected_category.clone();
    let initial_selected_sheet_name = state.selected_sheet_name.clone();

    editor_event_handling::process_editor_events_and_state(
        &mut state,
        &misc.registry,
        &misc.render_cache_res,
        &mut sheet_writers,
        &mut sheet_data_modified_events,
        &initial_selected_category,
        &initial_selected_sheet_name,
    );

    // Reload cache from DB if sheet was switched
    crate::sheets::systems::ui_handlers::sheet_handlers::reload_sheet_cache_from_db(
        &mut state,
        &mut misc.registry,
        misc.daemon_client.client(),
    );

    // Trigger revalidation when a sheet is opened/re-opened
    if state.pending_sheet_revalidation {
        state.pending_sheet_revalidation = false;
        if let Some(ref sheet_name) = state.selected_sheet_name {
            sheet_writers.revalidate.write(RequestSheetRevalidation {
                category: state.selected_category.clone(),
                sheet_name: sheet_name.clone(),
            });
        }
    }

    editor_popups_integration::display_active_popups(
        ctx,
        &mut state,
        &mut migration_state,
        &mut sheet_writers,
        &mut misc.registry,
        &misc.ui_feedback,
        &mut misc.api_key_status_res,
        &mut misc.session_api_key_res,
        &mut misc.copier_manager,
        &mut copier_writers.pick_folder,
        &mut copier_writers.queue_top_panel_copy,
        &mut copier_writers.reverse_folders,
        &mut copier_writers.state_changed,
        misc.daemon_client.client(),
    );

    // Bottom panels must be declared before CentralPanel to reserve space
    // Draw persistent Category/Sheet bar at window bottom (always the bottom-most)
    egui::TopBottomPanel::bottom("category_sheet_bottom_bar").show(ctx, |ui_b| {
        crate::ui::elements::bottom_panel::sheet_management_bar::show_sheet_management_controls(
            ui_b,
            &mut state,
            &mut *misc.registry,
            &mut crate::ui::elements::bottom_panel::sheet_management_bar::SheetManagementEventWriters {
                move_sheet_to_category: &mut sheet_writers.move_sheet_to_category,
            },
            misc.daemon_client.client(),
        );
    });
    // Draw Log panel above the category/sheet bar
    editor_ai_log::show_ai_output_log_bottom(ctx, &mut state);

    // Render central panel (main content)
    egui::CentralPanel::default().show(ctx, |ui| {
        // Prevent general back navigation (ESC) if inside AI Review
        let is_ai_reviewing = state.ai_mode == AiModeState::Reviewing;

        if !is_ai_reviewing && keys.just_pressed(KeyCode::Escape) {
            if let Some(nav_ctx) = state.structure_navigation_stack.pop() {
                // Navigate back to parent sheet in real navigation
                state.selected_category = nav_ctx.parent_category;
                state.selected_sheet_name = Some(nav_ctx.parent_sheet_name);
            }
        }
        // Fix row height to the checkbox/interact size so cell height never changes when the left checkbox appears
        let row_height = ui.style().spacing.interact_size.y;

        // Keep the top panel minimal (Back + App Exit + toolbars) and move category/sheet row down
        show_top_panel_orchestrator(
            ui,
            &mut state,
            &mut *misc.registry,
            &mut sheet_writers,
            misc.request_app_exit_writer,
            &misc.runtime,
            &misc.session_api_key_res,
            &mut commands,
            &mut *misc.director_session,
        );

        ui.add_space(10.0);

        let current_category_clone = state.selected_category.clone();
        let current_sheet_name_clone = state.selected_sheet_name.clone();

        // Mode panels are now drawn inline in the top controls (above the delimiter)

        if !(state.current_interaction_mode == SheetInteractionState::AiModeActive
            && state.ai_mode == AiModeState::Reviewing)
        {
            editor_sheet_display::show_sheet_table(
                ui,
                ctx,
                row_height,
                &mut state,
                &misc.registry,
                &misc.render_cache_res,
                sheet_writers.reorder_column,
                sheet_writers.cell_update,
                sheet_writers.toggle_ai_row_generation,
                sheet_writers.update_column_ai_include,
                sheet_writers.batch_update_column_ai_include,
                sheet_writers.update_ai_send_schema,
                sheet_writers.update_ai_structure_send,
                sheet_writers.add_row,
                sheet_writers.add_column,
                sheet_writers.copy_cell,
                sheet_writers.paste_cell,
                &misc.clipboard_buffer,
            );
        } else {
            // Show review panel when in AI Reviewing state
            // The review panel is self-contained and shows all necessary context (ancestor keys, row data)
            // Do NOT show the virtual sheet table here - it causes confusing double-UI during review
            crate::ui::elements::ai_review::ai_batch_review_ui::draw_ai_batch_review_panel(
                ui,
                &mut state,
                &current_category_clone,
                &current_sheet_name_clone,
                &misc.registry,
                &mut sheet_writers.cell_update,
                &mut sheet_writers.add_row,
            );
        }

        // (moved to a global bottom panel below this CentralPanel block)

        // AI output bottom panel rendered after main content outside this closure
    });

    // (panels already drawn above CentralPanel)
}
