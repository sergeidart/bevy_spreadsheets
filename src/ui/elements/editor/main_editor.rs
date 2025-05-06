// src/ui/elements/editor/main_editor.rs
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use bevy_tokio_tasks::TokioTasksRuntime;
use egui_extras::{Column, TableBody, TableBuilder};

use crate::sheets::{
    definitions::{ColumnDefinition, SheetMetadata}, // SheetMetadata needed
    events::{
        AddSheetRowRequest, RequestDeleteSheet, RequestInitiateFileUpload,
        RequestRenameSheet, RequestUpdateColumnName, RequestUpdateColumnValidator,
        UpdateCellEvent, RequestDeleteRows, RequestUpdateColumnWidth,
        SheetDataModifiedInRegistryEvent, // Import new event
    },
    resources::SheetRegistry,
};
use crate::ui::{
    elements::{
        popups::{
            show_ai_rule_popup, show_column_options_popup,
            show_delete_confirm_popup, show_rename_popup, show_settings_popup,
        },
        top_panel::show_top_panel,
    },
    UiFeedbackState,
};
use super::state::{AiModeState, EditorWindowState};
use super::table_body::sheet_table_body;
use super::table_header::sheet_table_header;
use super::ai_control_panel::show_ai_control_panel;
use super::ai_review_ui::show_ai_review_ui;

#[allow(clippy::too_many_arguments)]
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: Local<EditorWindowState>,
    mut add_row_event_writer: EventWriter<AddSheetRowRequest>, // Keep mutable for sending
    mut rename_event_writer: EventWriter<RequestRenameSheet>,
    mut delete_event_writer: EventWriter<RequestDeleteSheet>,
    mut upload_req_writer: EventWriter<RequestInitiateFileUpload>, // Keep mutable
    mut column_rename_writer: EventWriter<RequestUpdateColumnName>,
    mut column_validator_writer: EventWriter<RequestUpdateColumnValidator>,
    mut cell_update_writer: EventWriter<UpdateCellEvent>,
    mut delete_rows_writer: EventWriter<RequestDeleteRows>, // Keep mutable
    mut registry: ResMut<SheetRegistry>, // Keep mutable for popups that modify it
    ui_feedback: Res<UiFeedbackState>,
    runtime: Res<TokioTasksRuntime>,
    mut commands: Commands,
    mut sheet_data_modified_events: EventReader<SheetDataModifiedInRegistryEvent>, // Read modification events
    // Removed RequestUpdateColumnWidth writer as it's not directly used here but by logic systems
) {
    let ctx = contexts.ctx_mut();
    // Clone selected sheet identifiers for use in closures/popups if needed
    // These are small and cloning them is fine.
    let selected_category_clone = state.selected_category.clone();
    let selected_sheet_name_clone = state.selected_sheet_name.clone();

    // Handle sheet data modification events for cache invalidation
    for event in sheet_data_modified_events.read() {
        if state.selected_category == event.category && state.selected_sheet_name.as_ref() == Some(&event.sheet_name) {
            debug!("Received SheetDataModifiedInRegistryEvent for current sheet '{:?}/{}'. Forcing filter recalc.", event.category, event.sheet_name);
            state.force_filter_recalculation = true;
        }
    }


    show_column_options_popup(
        ctx,
        &mut state,
        &mut column_rename_writer,
        &mut column_validator_writer,
        &mut registry, // Pass ResMut registry
    );
    show_rename_popup(ctx, &mut state, &mut rename_event_writer, &ui_feedback);
    show_delete_confirm_popup(ctx, &mut state, &mut delete_event_writer);
    show_ai_rule_popup(ctx, &mut state, &mut registry); // Pass ResMut registry
    show_settings_popup(ctx, &mut state);

    egui::CentralPanel::default().show(ctx, |ui| {
        let text_style = egui::TextStyle::Body;
        let row_height = ui.text_style_height(&text_style)
            + ui.style().spacing.item_spacing.y;

        // Pass immutable registry to top_panel as it only reads
        show_top_panel(
            ui,
            &mut state,
            &registry, // Pass immutable reference
            add_row_event_writer, // Pass the writer directly
            upload_req_writer,    // Pass the writer directly
            delete_rows_writer,   // Pass the writer directly
        );
        

        if !ui_feedback.last_message.is_empty() {
            let text_color = if ui_feedback.is_error {
                egui::Color32::RED
            } else {
                ui.style().visuals.text_color()
            };
            ui.colored_label(text_color, &ui_feedback.last_message);
        }
        ui.separator();

        if state.ai_mode != AiModeState::Idle && state.ai_mode != AiModeState::Reviewing {
             show_ai_control_panel(
                 ui,
                 &mut state,
                 &selected_category_clone, // These are Options, cloning is cheap
                 &selected_sheet_name_clone,
                 &runtime, // Pass immutable reference
                 &registry, // Pass immutable reference
                 &mut commands,
             );
             ui.separator();
        }

        if let Some(selected_name) = &selected_sheet_name_clone { // Use cloned name
            if state.ai_mode == AiModeState::Reviewing {
                show_ai_review_ui(
                    ui,
                    &mut state,
                    &selected_category_clone, // Pass cloned category
                    &selected_sheet_name_clone, 
                    &registry, // Pass immutable reference
                    &mut cell_update_writer,
                );
            } else {
                // --- MODIFIED: Get a reference, not a clone ---
                let sheet_data_ref_opt = registry.get_sheet(&selected_category_clone, selected_name);

                if sheet_data_ref_opt.is_none() {
                    warn!(
                        "Selected sheet '{:?}/{}' not found in registry for rendering.",
                        selected_category_clone, selected_name
                    );
                    ui.vertical_centered(|ui| {
                        ui.label(format!(
                            "Sheet '{:?}/{}' no longer exists...",
                            selected_category_clone, selected_name
                        ));
                    });
                    // If the currently selected sheet in state matches the one that disappeared
                    if state.selected_sheet_name.as_deref() == Some(selected_name.as_str()) {
                        state.selected_sheet_name = None;
                        state.ai_selected_rows.clear();
                        state.ai_mode = AiModeState::Idle;
                        state.force_filter_recalculation = true; // Invalidate cache
                    }
                } else if let Some(sheet_data_ref) = sheet_data_ref_opt { // Now an Option<&SheetGridData>
                    if let Some(metadata) = &sheet_data_ref.metadata { // Access metadata via reference
                        let headers = metadata.get_headers();
                        let filters = metadata.get_filters(); // This is cheap (clones Vec<Option<String>>)
                        let num_cols = metadata.columns.len();
                        debug!(
                            "Preparing table: num_cols = {}, headers = {:?}, filters = {:?}",
                            num_cols, headers, filters
                        );
                        if num_cols != filters.len() {
                            ui.colored_label(
                                egui::Color32::RED,
                                "Metadata inconsistency detected...",
                            );
                            return;
                        }

                        egui::ScrollArea::both()
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                let mut table_builder = TableBuilder::new(ui)
                                    .striped(true)
                                    .resizable(true) 
                                    .cell_layout(egui::Layout::left_to_right(
                                        egui::Align::Center,
                                    ))
                                    .min_scrolled_height(0.0);

                                if num_cols == 0 {
                                    table_builder = table_builder
                                        .column(Column::remainder().resizable(false));
                                } else {
                                    for i in 0..num_cols {
                                        let initial_width = metadata.columns.get(i)
                                            .and_then(|c| c.width)
                                            .unwrap_or(120.0); 

                                        let col = Column::initial(initial_width)
                                            .at_least(40.0)
                                            .resizable(true) 
                                            .clip(true);
                                        table_builder = table_builder.column(col);
                                    }
                                }

                                table_builder
                                    .header(20.0, |mut header_row| {
                                        // sheet_table_header needs access to metadata, but doesn't need to own it
                                        sheet_table_header(
                                            header_row,
                                            metadata, // Pass reference to metadata
                                            selected_name, // sheet_name is &String
                                            &mut state,
                                        );
                                        if state.show_column_options_popup
                                            && state.column_options_popup_needs_init
                                        {
                                            state.options_column_target_category =
                                                selected_category_clone.clone();
                                        }
                                    })
                                    .body(|body: TableBody| {
                                        // sheet_table_body takes immutable registry and sheet identifiers
                                        sheet_table_body(
                                            body,
                                            row_height,
                                            &selected_category_clone, // Pass cloned category
                                            selected_name, // sheet_name is &String
                                            &registry, // Pass immutable reference to registry
                                            cell_update_writer, // Pass writer
                                            &mut state,
                                        );
                                    });
                            }); 
                    } else {
                        warn!(
                            "Metadata object missing for sheet '{:?}/{}' even though sheet data exists.",
                            selected_category_clone, selected_name
                        );
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!(
                                "Metadata missing for sheet '{:?}/{}'.",
                                selected_category_clone, selected_name
                            ),
                        );
                    }
                } // else if sheet_data_ref_opt end
            } // else (not reviewing) end
        } else { // No sheet selected
            if state.selected_category.is_some() {
                ui.vertical_centered(|ui| {
                    ui.label("Select a sheet from the category.");
                });
            } else {
                ui.vertical_centered(|ui| {
                    ui.label("Select a category and sheet, or upload JSON.");
                });
            }
        }
    }); // End CentralPanel
}