// src/ui/elements/editor/main_editor.rs
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use bevy_tokio_tasks::TokioTasksRuntime;

use crate::sheets::{
    events::{
        AddSheetRowRequest, RequestDeleteSheet, RequestInitiateFileUpload, RequestRenameSheet,
        RequestUpdateColumnName, RequestUpdateColumnValidator, UpdateCellEvent, RequestDeleteRows,
        RequestSheetRevalidation, SheetDataModifiedInRegistryEvent, RequestDeleteColumns,
        RequestAddColumn, RequestReorderColumn, RequestCreateNewSheet,
    },
    resources::{SheetRegistry, SheetRenderCache},
};
use crate::ui::{
    elements::top_panel::show_top_panel_orchestrator,
    UiFeedbackState,
};
use super::state::{AiModeState, EditorWindowState, SheetInteractionState};
use super::editor_event_handling;
use super::editor_popups_integration;
use super::editor_mode_panels;
use super::editor_sheet_display;
use super::editor_ai_log;

use crate::ApiKeyDisplayStatus;
use crate::SessionApiKey;
use crate::visual_copier::{
    resources::VisualCopierManager,
    events::{
        PickFolderRequest, QueueTopPanelCopyEvent, ReverseTopPanelFoldersEvent,
        VisualCopierStateChanged,
        RequestAppExit,
    },
};

#[derive(SystemParam)]
pub struct SheetEventWriters<'w> {
    pub add_row: EventWriter<'w, AddSheetRowRequest>,
    pub add_column: EventWriter<'w, RequestAddColumn>,
    pub create_sheet: EventWriter<'w, RequestCreateNewSheet>,
    pub rename_sheet: EventWriter<'w, RequestRenameSheet>,
    pub delete_sheet: EventWriter<'w, RequestDeleteSheet>,
    pub upload_req: EventWriter<'w, RequestInitiateFileUpload>,
    pub column_rename: EventWriter<'w, RequestUpdateColumnName>,
    pub column_validator: EventWriter<'w, RequestUpdateColumnValidator>,
    pub cell_update: EventWriter<'w, UpdateCellEvent>,
    pub delete_rows: EventWriter<'w, RequestDeleteRows>,
    pub delete_columns: EventWriter<'w, RequestDeleteColumns>,
    pub reorder_column: EventWriter<'w, RequestReorderColumn>,
    pub revalidate: EventWriter<'w, RequestSheetRevalidation>,
}

#[derive(SystemParam)]
pub struct CopierEventWriters<'w> {
    pub pick_folder: EventWriter<'w, PickFolderRequest>,
    pub queue_top_panel_copy: EventWriter<'w, QueueTopPanelCopyEvent>,
    pub reverse_folders: EventWriter<'w, ReverseTopPanelFoldersEvent>,
    pub state_changed: EventWriter<'w, VisualCopierStateChanged>,
}


#[allow(clippy::too_many_arguments)]
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorWindowState>,
    mut sheet_writers: SheetEventWriters,
    copier_writers: CopierEventWriters,
    mut registry: ResMut<SheetRegistry>,
    render_cache_res: Res<SheetRenderCache>,
    ui_feedback: Res<UiFeedbackState>,
    runtime: Res<TokioTasksRuntime>,
    mut commands: Commands,
    mut sheet_data_modified_events: EventReader<SheetDataModifiedInRegistryEvent>,
    mut api_key_status_res: ResMut<ApiKeyDisplayStatus>,
    mut session_api_key_res: ResMut<SessionApiKey>,
    copier_manager: ResMut<VisualCopierManager>,
    request_app_exit_writer: EventWriter<RequestAppExit>,
) {
    let ctx = contexts.ctx_mut();
    let initial_selected_category = state.selected_category.clone();
    let initial_selected_sheet_name = state.selected_sheet_name.clone();

    editor_event_handling::process_editor_events_and_state(
        &mut state,
        &registry,
        &render_cache_res,
        &mut sheet_writers,
        &mut sheet_data_modified_events,
        &initial_selected_category,
        &initial_selected_sheet_name,
    );

    editor_popups_integration::display_active_popups(
        ctx,
        &mut state,
        &mut sheet_writers,
        &mut registry,
        &ui_feedback,
        &mut api_key_status_res,
        &mut session_api_key_res,
    );

    egui::CentralPanel::default().show(ctx, |ui| {
        let text_style = egui::TextStyle::Body;
        let row_height = ui.text_style_height(&text_style) + ui.style().spacing.item_spacing.y;

        show_top_panel_orchestrator(
            ui,
            &mut state,
            &mut *registry,
            // MODIFIED: Pass &mut sheet_writers
            &mut sheet_writers,
            copier_manager, // Assuming this is ResMut or similar, passed correctly
            copier_writers.pick_folder, // Individual writers are Copy
            copier_writers.queue_top_panel_copy,
            copier_writers.reverse_folders,
            request_app_exit_writer, // This is an EventWriter, Copy
            copier_writers.state_changed,
        );

        ui.add_space(10.0);

        if !ui_feedback.last_message.is_empty() {
            let text_color = if ui_feedback.is_error { egui::Color32::RED } else { ui.style().visuals.text_color() };
            ui.colored_label(text_color, &ui_feedback.last_message);
        }

        let current_category_clone = state.selected_category.clone();
        let current_sheet_name_clone = state.selected_sheet_name.clone();

        editor_mode_panels::show_active_mode_panel(
            ui,
            &mut state,
            &current_category_clone,
            &current_sheet_name_clone,
            &runtime,
            &registry,
            &mut commands,
            &session_api_key_res,
            &mut sheet_writers,
        );

        if !(state.current_interaction_mode == SheetInteractionState::AiModeActive && state.ai_mode == AiModeState::Reviewing) {
            editor_sheet_display::show_sheet_table(
                ui,
                ctx,
                row_height,
                &mut state,
                &registry,
                &render_cache_res,
                sheet_writers.reorder_column,
                sheet_writers.cell_update,
            );
        }

        editor_ai_log::show_ai_output_log(ui, &state);
    });
}