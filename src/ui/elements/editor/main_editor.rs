// src/ui/elements/editor/main_editor.rs
// Added AppExit
use bevy::{app::AppExit, ecs::system::SystemParam, prelude::*};
use bevy_egui::{egui, EguiContexts};
use bevy_tokio_tasks::TokioTasksRuntime;
use egui_extras::{Column, TableBody, TableBuilder};
use crate::sheets::{
    definitions::SheetMetadata,
    events::{AddSheetRowRequest, RequestDeleteSheet, RequestInitiateFileUpload, RequestRenameSheet, RequestUpdateColumnName, RequestUpdateColumnValidator, UpdateCellEvent, RequestDeleteRows, RequestSheetRevalidation, SheetDataModifiedInRegistryEvent},
    resources::{SheetRegistry, SheetRenderCache},
};
use crate::ui::{
    elements::{
        popups::{
            show_ai_rule_popup, show_column_options_popup,
            show_delete_confirm_popup, show_rename_popup, show_settings_popup,
        },
        top_panel::{show_top_panel, show_delete_row_control_panel},
    },
    UiFeedbackState,
};
use super::state::{AiModeState, EditorWindowState};
use super::table_body::sheet_table_body;
use super::table_header::sheet_table_header;
use super::ai_control_panel::show_ai_control_panel;
use super::ai_review_ui::draw_inline_ai_review_panel;
use super::ai_helpers;
use crate::ApiKeyDisplayStatus;
use crate::SessionApiKey;

use crate::visual_copier::{
    resources::VisualCopierManager,
    events::{
        PickFolderRequest, QueueTopPanelCopyEvent, ReverseTopPanelFoldersEvent,
    },
};


// SystemParam for sheet-related event writers
#[derive(SystemParam)]
pub struct SheetEventWriters<'w, 's> {
    add_row: EventWriter<'w, AddSheetRowRequest>,
    rename_sheet: EventWriter<'w, RequestRenameSheet>,
    delete_sheet: EventWriter<'w, RequestDeleteSheet>,
    upload_req: EventWriter<'w, RequestInitiateFileUpload>,
    column_rename: EventWriter<'w, RequestUpdateColumnName>,
    column_validator: EventWriter<'w, RequestUpdateColumnValidator>,
    cell_update: EventWriter<'w, UpdateCellEvent>,
    delete_rows: EventWriter<'w, RequestDeleteRows>,
    revalidate: EventWriter<'w, RequestSheetRevalidation>,
    _marker: std::marker::PhantomData<&'s ()>,
}

// SystemParam for visual copier event writers
#[derive(SystemParam)]
pub struct CopierEventWriters<'w, 's> {
    pick_folder: EventWriter<'w, PickFolderRequest>,
    queue_top_panel_copy: EventWriter<'w, QueueTopPanelCopyEvent>,
    reverse_folders: EventWriter<'w, ReverseTopPanelFoldersEvent>,
    _marker: std::marker::PhantomData<&'s ()>,
}


#[allow(clippy::too_many_arguments)]
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorWindowState>,
    mut sheet_writers: SheetEventWriters,
    mut registry: ResMut<SheetRegistry>,
    render_cache_res: Res<SheetRenderCache>,
    ui_feedback: Res<UiFeedbackState>,
    runtime: Res<TokioTasksRuntime>,
    mut commands: Commands,
    mut sheet_data_modified_events: EventReader<SheetDataModifiedInRegistryEvent>,
    mut api_key_status_res: ResMut<ApiKeyDisplayStatus>,
    mut session_api_key_res: ResMut<SessionApiKey>,
    mut copier_manager: ResMut<VisualCopierManager>,
    mut copier_writers: CopierEventWriters,
    // Added app_exit_writer
    mut app_exit_writer: EventWriter<AppExit>,
) {

    let ctx = contexts.ctx_mut();
    let initial_selected_category = state.selected_category.clone();
    let initial_selected_sheet_name = state.selected_sheet_name.clone();

    // --- Event handling for sheet data modification ---
    // (Remains the same)
    for event in sheet_data_modified_events.read() {
        if state.selected_category == event.category && state.selected_sheet_name.as_ref() == Some(&event.sheet_name) {
            debug!("main_editor: Received SheetDataModifiedInRegistryEvent for current sheet '{:?}/{}'. Forcing filter recalc.", event.category, event.sheet_name);
            state.force_filter_recalculation = true;
            if state.request_scroll_to_bottom_on_add {
                if let Some(sheet_data) = registry.get_sheet(&event.category, &event.sheet_name) {
                    let new_row_count = sheet_data.grid.len();
                    if new_row_count > 0 { state.scroll_to_row_index = Some(new_row_count - 1); }
                }
                state.request_scroll_to_bottom_on_add = false;
            }
            if render_cache_res.get_cell_data(&event.category, &event.sheet_name, 0, 0).is_none()
                && registry.get_sheet(&event.category, &event.sheet_name).map_or(false, |d| !d.grid.is_empty()) {
                 sheet_writers.revalidate.send(RequestSheetRevalidation { category: event.category.clone(), sheet_name: event.sheet_name.clone() });
            }
        }
    }

    // --- Popups ---
    // (Remain the same)
    show_column_options_popup(ctx, &mut state, &mut sheet_writers.column_rename, &mut sheet_writers.column_validator, &mut registry);
    show_rename_popup(ctx, &mut state, &mut sheet_writers.rename_sheet, &ui_feedback);
    show_delete_confirm_popup(ctx, &mut state, &mut sheet_writers.delete_sheet);
    show_ai_rule_popup(ctx, &mut state, &mut registry);
    show_settings_popup(ctx, &mut state, &mut api_key_status_res, &mut session_api_key_res);


    egui::CentralPanel::default().show(ctx, |ui| {
        let text_style = egui::TextStyle::Body;
        let row_height = ui.text_style_height(&text_style) + ui.style().spacing.item_spacing.y;

        // --- Top Panel ---
        show_top_panel(
            ui,
            &mut state,
            &registry,
            sheet_writers.add_row,
            sheet_writers.upload_req,
            // delete_rows writer removed from here
            copier_manager,
            copier_writers.pick_folder,
            copier_writers.queue_top_panel_copy,
            copier_writers.reverse_folders,
            // Pass app_exit_writer
            app_exit_writer,
        );

        // --- Rest of the UI logic ---
        // (Sheet change detection remains the same)
        if initial_selected_category != state.selected_category || initial_selected_sheet_name != state.selected_sheet_name {
            debug!("Selected sheet or category changed by UI interaction.");
            if let Some(sheet_name) = &state.selected_sheet_name {
                if render_cache_res.get_cell_data(&state.selected_category, sheet_name, 0, 0).is_none()
                    && registry.get_sheet(&state.selected_category, sheet_name).map_or(false, |d| !d.grid.is_empty()) {
                    sheet_writers.revalidate.send(RequestSheetRevalidation { category: state.selected_category.clone(), sheet_name: sheet_name.clone() });
                }
            }
            state.force_filter_recalculation = true;
        }

        // (Feedback display remains the same)
        if !ui_feedback.last_message.is_empty() {
            let text_color = if ui_feedback.is_error { egui::Color32::RED } else { ui.style().visuals.text_color() };
            ui.colored_label(text_color, &ui_feedback.last_message);
        }

        // --- Conditional Control Panels ---
        let current_category_clone = state.selected_category.clone();
        let current_sheet_name_clone = state.selected_sheet_name.clone();
        let current_ai_mode = state.ai_mode;
        let delete_row_mode_active = state.delete_row_mode_active; // Cache state

        let mut control_panel_shown = false; // Track if any control panel was shown

        // Show AI Control Panel
        if matches!(current_ai_mode, AiModeState::Preparing | AiModeState::Submitting | AiModeState::ResultsReady) {
            ui.separator();
            show_ai_control_panel(
                ui,
                &mut state,
                &current_category_clone,
                &current_sheet_name_clone,
                &runtime,
                &registry,
                &mut commands,
                &session_api_key_res,
            );
            control_panel_shown = true;
         }

        // Show Delete Row Control Panel
        if delete_row_mode_active {
             // Avoid double separator if AI panel also shown (unlikely but possible)
             if !control_panel_shown { ui.separator(); }
             show_delete_row_control_panel(
                 ui,
                 &mut state,
                 sheet_writers.delete_rows, // Pass the writer here
             );
             control_panel_shown = true;
        }

        // Add separator after control panels if either was shown
        if control_panel_shown {
            ui.separator();
        }


        // --- AI Review Panel or Main Table ---
        // (Remains the same)
        if current_ai_mode == AiModeState::Reviewing {
            if current_sheet_name_clone.is_some() {
                 draw_inline_ai_review_panel(ui, &mut state, &current_category_clone, &current_sheet_name_clone, &registry, &mut sheet_writers.cell_update);
                 ui.add_space(5.0);
             } else {
                 warn!("In Review Mode but no sheet selected. Exiting review mode.");
                 ai_helpers::exit_review_mode(&mut state);
             }
         }

        if current_ai_mode != AiModeState::Reviewing {
            if let Some(selected_name) = &current_sheet_name_clone {
                let sheet_data_ref_opt = registry.get_sheet(&current_category_clone, selected_name);
                if sheet_data_ref_opt.is_none() {
                    warn!("Selected sheet '{:?}/{}' not found in registry for rendering.", current_category_clone, selected_name);
                    ui.vertical_centered(|ui| { ui.label(format!("Sheet '{:?}/{}' no longer exists...", current_category_clone, selected_name)); });
                    if state.selected_sheet_name.as_deref() == Some(selected_name.as_str()) {
                        state.selected_sheet_name = None;
                        ai_helpers::exit_review_mode(&mut state);
                        state.force_filter_recalculation = true;
                    }
                } else if let Some(sheet_data_ref) = sheet_data_ref_opt {
                     if let Some(metadata) = &sheet_data_ref.metadata {
                        let num_cols = metadata.columns.len();
                        if metadata.get_filters().len() != num_cols && num_cols > 0 {
                             ui.colored_label(egui::Color32::RED, "Metadata inconsistency detected (cols vs filters)...");
                             return;
                        }
                        egui::ScrollArea::both()
                            .id_salt("main_sheet_table_scroll_area")
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                               let mut table_builder = TableBuilder::new(ui)
                                   .striped(true)
                                   .resizable(true)
                                   .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                   .min_scrolled_height(0.0);
                               if num_cols == 0 {
                                    if state.scroll_to_row_index.is_some() { state.scroll_to_row_index = None; }
                                    table_builder = table_builder.column(Column::remainder().resizable(false));
                               } else {
                                    for i in 0..num_cols {
                                        let initial_width = metadata.columns.get(i).and_then(|c| c.width).unwrap_or(120.0);
                                        let col = Column::initial(initial_width).at_least(40.0).resizable(true).clip(true);
                                        table_builder = table_builder.column(col);
                                    }
                               }
                               if let Some(row_idx) = state.scroll_to_row_index {
                                    if num_cols > 0 { table_builder = table_builder.scroll_to_row(row_idx, Some(egui::Align::BOTTOM)); }
                                    state.scroll_to_row_index = None;
                               }
                               table_builder
                                   .header(20.0, |header_row| { sheet_table_header(header_row, metadata, selected_name, &mut state); })
                                   .body(|body: TableBody| { sheet_table_body(body, row_height, &current_category_clone, selected_name, &registry, &render_cache_res, sheet_writers.cell_update, &mut state); });
                            });
                    } else {
                         warn!("Metadata object missing for sheet '{:?}/{}' even though sheet data exists.", current_category_clone, selected_name);
                         ui.colored_label(egui::Color32::YELLOW, format!("Metadata missing for sheet '{:?}/{}'.", current_category_clone, selected_name));
                    }
                }
            } else {
                 if current_category_clone.is_some() { ui.vertical_centered(|ui| { ui.label("Select a sheet from the category."); }); }
                 else { ui.vertical_centered(|ui| { ui.label("Select a category and sheet, or upload JSON."); }); }
            }
        }

        // --- AI Output Log ---
        // (Remains the same)
        ui.separator();
        ui.strong("AI Output / Log:");
        egui::ScrollArea::vertical()
            .id_salt("ai_raw_output_log_scroll_area")
            .max_height(100.0)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let mut display_text_clone = state.ai_raw_output_display.clone();
                ui.add_sized(
                    ui.available_size(),
                    egui::TextEdit::multiline(&mut display_text_clone)
                        .font(egui::TextStyle::Monospace)
                        .interactive(false)
                        .desired_width(f32::INFINITY)
                );
            });
    }); // End CentralPanel
}