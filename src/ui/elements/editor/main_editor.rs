// src/ui/elements/editor/main_editor.rs
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use bevy::input::keyboard::KeyCode;
use bevy::window::Window;
use bevy_tokio_tasks::TokioTasksRuntime;

use crate::sheets::{
    events::{
        AddSheetRowRequest, RequestDeleteSheet, RequestInitiateFileUpload, RequestRenameSheet,
        RequestUpdateColumnName, RequestUpdateColumnValidator, UpdateCellEvent, RequestDeleteRows,
        RequestSheetRevalidation, SheetDataModifiedInRegistryEvent, RequestDeleteColumns,
        RequestAddColumn, RequestReorderColumn, RequestCreateNewSheet, CloseStructureViewEvent,
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
    pub open_structure: EventWriter<'w, crate::sheets::events::OpenStructureViewEvent>,
}

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
    pub ui_feedback: Res<'w, UiFeedbackState>,
    pub runtime: Res<'w, TokioTasksRuntime>,
    pub api_key_status_res: ResMut<'w, ApiKeyDisplayStatus>,
    pub session_api_key_res: ResMut<'w, SessionApiKey>,
    pub copier_manager: ResMut<'w, VisualCopierManager>,
    pub request_app_exit_writer: EventWriter<'w, RequestAppExit>,
    pub close_structure_writer: EventWriter<'w, CloseStructureViewEvent>,
}


#[allow(clippy::too_many_arguments)]
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorWindowState>,
    mut sheet_writers: SheetEventWriters,
    copier_writers: CopierEventWriters,
    mut misc: EditorMiscParams,
    mut commands: Commands,
    mut sheet_data_modified_events: EventReader<SheetDataModifiedInRegistryEvent>,
    window_query: Query<Entity, With<Window>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    // Guard: If all windows are closed (app shutting down) skip egui usage to avoid panic
    if window_query.is_empty() { return; }
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

    editor_popups_integration::display_active_popups(
        ctx,
        &mut state,
        &mut sheet_writers,
        &mut misc.registry,
        &misc.ui_feedback,
        &mut misc.api_key_status_res,
        &mut misc.session_api_key_res,
    );

    // Render central panel (main content) first
    egui::CentralPanel::default().show(ctx, |ui| {
        if keys.just_pressed(KeyCode::Escape) && !state.virtual_structure_stack.is_empty() {
            misc.close_structure_writer.write(CloseStructureViewEvent);
        }
        let text_style = egui::TextStyle::Body;
        let row_height = ui.text_style_height(&text_style) + ui.style().spacing.item_spacing.y;

        show_top_panel_orchestrator(
            ui,
            &mut state,
            &mut *misc.registry,
            &mut sheet_writers,
            misc.copier_manager,
            copier_writers.pick_folder,
            copier_writers.queue_top_panel_copy,
            copier_writers.reverse_folders,
            misc.request_app_exit_writer,
            copier_writers.state_changed,
            misc.close_structure_writer,
        );

        ui.add_space(10.0);

        if !misc.ui_feedback.last_message.is_empty() {
            let text_color = if misc.ui_feedback.is_error { egui::Color32::RED } else { ui.style().visuals.text_color() };
            ui.colored_label(text_color, &misc.ui_feedback.last_message);
        }

        let current_category_clone = state.selected_category.clone();
        let current_sheet_name_clone = state.selected_sheet_name.clone();

        editor_mode_panels::show_active_mode_panel(
            ui,
            &mut state,
            &current_category_clone,
            &current_sheet_name_clone,
            &misc.runtime,
            &misc.registry,
            &mut commands,
            &misc.session_api_key_res,
            &mut sheet_writers,
        );

        if !(state.current_interaction_mode == SheetInteractionState::AiModeActive && state.ai_mode == AiModeState::Reviewing) {
            editor_sheet_display::show_sheet_table(
                ui,
                ctx,
                row_height,
                &mut state,
                &misc.registry,
                &misc.render_cache_res,
                sheet_writers.reorder_column,
                sheet_writers.cell_update,
                sheet_writers.open_structure,
            );
        }

        // AI output bottom panel rendered after main content outside this closure
    });

    // Auto-hide logic: if context (category, sheet, structure presence) changed, hide panel
    let in_structure = !state.virtual_structure_stack.is_empty();
    let current_sheet_key = state.selected_sheet_name.clone();
    if let Some(sheet_name) = current_sheet_key.clone() {
        let current_ctx_tuple = (state.selected_category.clone(), sheet_name.clone(), in_structure);
        if let Some(last_ctx) = &state.ai_output_panel_last_context {
            if last_ctx != &current_ctx_tuple {
                // Different sheet or structure transition: hide panel
                state.ai_output_panel_visible = false;
            }
        }
        state.ai_output_panel_last_context = Some(current_ctx_tuple);
    } else {
        state.ai_output_panel_visible = false;
        state.ai_output_panel_last_context = None;
    }

    // Finally draw bottom panel if visible
    editor_ai_log::show_ai_output_log_bottom(ctx, &mut state);
}