// src/ui/elements/editor/main_editor.rs
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_egui::{egui, EguiContexts};
use bevy_tokio_tasks::TokioTasksRuntime;
use egui_extras::{Column, TableBody, TableBuilder};

use crate::sheets::{
    definitions::{ColumnDefinition, SheetMetadata},
    events::{
        AddSheetRowRequest, RequestDeleteSheet, RequestInitiateFileUpload,
        RequestRenameSheet, RequestUpdateColumnName, RequestUpdateColumnValidator,
        UpdateCellEvent, RequestDeleteRows, RequestUpdateColumnWidth,
        SheetDataModifiedInRegistryEvent, RequestSheetRevalidation, AiTaskResult,
    },
    resources::{SheetRegistry, SheetRenderCache},
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


#[allow(clippy::too_many_arguments)]
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: Local<EditorWindowState>,
    mut sheet_writers: SheetEventWriters,
    mut registry: ResMut<SheetRegistry>,
    render_cache_res: Res<SheetRenderCache>,
    ui_feedback: Res<UiFeedbackState>,
    runtime: Res<TokioTasksRuntime>,
    mut commands: Commands,
    mut sheet_data_modified_events: EventReader<SheetDataModifiedInRegistryEvent>,
) {
    let ctx = contexts.ctx_mut();

    let initial_selected_category = state.selected_category.clone();
    let initial_selected_sheet_name = state.selected_sheet_name.clone();

    for event in sheet_data_modified_events.read() {
        if state.selected_category == event.category && state.selected_sheet_name.as_ref() == Some(&event.sheet_name) {
            debug!("main_editor: Received SheetDataModifiedInRegistryEvent for current sheet '{:?}/{}'. Forcing filter recalc.", event.category, event.sheet_name);
            state.force_filter_recalculation = true;

            if render_cache_res.get_cell_data(&event.category, &event.sheet_name, 0, 0).is_none()
                && registry.get_sheet(&event.category, &event.sheet_name).map_or(false, |d| !d.grid.is_empty())
            {
                 info!("Render cache seems to be missing for modified sheet '{:?}/{}'. Explicitly requesting revalidation/cache update.", event.category, event.sheet_name);
                 sheet_writers.revalidate.send(RequestSheetRevalidation {
                    category: event.category.clone(),
                    sheet_name: event.sheet_name.clone(),
                });
            }
        }
    }

    // These popups modify state or registry, call them before the main panel potentially borrows state mutably again
    show_column_options_popup( ctx, &mut state, &mut sheet_writers.column_rename, &mut sheet_writers.column_validator, &mut registry, );
    show_rename_popup(ctx, &mut state, &mut sheet_writers.rename_sheet, &ui_feedback);
    show_delete_confirm_popup(ctx, &mut state, &mut sheet_writers.delete_sheet);
    show_ai_rule_popup(ctx, &mut state, &mut registry);
    show_settings_popup(ctx, &mut state);

    egui::CentralPanel::default().show(ctx, |ui| {
        let text_style = egui::TextStyle::Body;
        let row_height = ui.text_style_height(&text_style)
            + ui.style().spacing.item_spacing.y;

        // --- Top Panel (Can modify state) ---
        show_top_panel(
            ui,
            &mut state, // Mutable borrow #1
            &registry, // Immutable borrow (fine)
            sheet_writers.add_row,
            sheet_writers.upload_req,
            sheet_writers.delete_rows,
        ); // Mutable borrow #1 ends here

        // --- Check for selection change AFTER top_panel ---
        if initial_selected_category != state.selected_category || initial_selected_sheet_name != state.selected_sheet_name {
            debug!("Selected sheet or category changed by UI interaction.");
            if let Some(sheet_name) = &state.selected_sheet_name {
                if render_cache_res.get_cell_data(&state.selected_category, sheet_name, 0, 0).is_none()
                    && registry.get_sheet(&state.selected_category, sheet_name).map_or(false, |d| !d.grid.is_empty())
                {
                    debug!("Newly selected sheet '{:?}/{}' may not be in render cache. Requesting update.", state.selected_category, sheet_name);
                    sheet_writers.revalidate.send(RequestSheetRevalidation {
                        category: state.selected_category.clone(),
                        sheet_name: sheet_name.clone(),
                    });
                }
            }
            state.force_filter_recalculation = true;
        }


        if !ui_feedback.last_message.is_empty() {
            let text_color = if ui_feedback.is_error { egui::Color32::RED } else { ui.style().visuals.text_color() };
            ui.colored_label(text_color, &ui_feedback.last_message);
        }
        ui.separator();

        // --- Clone selection state BEFORE potential mutable borrows for AI panels ---
        let current_category_clone = state.selected_category.clone();
        let current_sheet_name_clone = state.selected_sheet_name.clone();

         if state.ai_mode != AiModeState::Idle && state.ai_mode != AiModeState::Reviewing {
             // Pass cloned values for immutable parts
             show_ai_control_panel(
                 ui,
                 &mut state, // Mutable borrow #2
                 &current_category_clone, // Pass clone
                 &current_sheet_name_clone, // Pass clone
                 &runtime,
                 &registry,
                 &mut commands,
             ); // Mutable borrow #2 ends here
             ui.separator();
        }


        // Use the *cloned* values for checks and immutable access within this block
        if let Some(selected_name) = &current_sheet_name_clone { // Use clone
            if state.ai_mode == AiModeState::Reviewing {
                 // Pass cloned values for immutable parts
                 show_ai_review_ui(
                     ui,
                     &mut state, // Mutable borrow #3
                     &current_category_clone, // Pass clone
                     &current_sheet_name_clone, // Pass clone
                     &registry,
                     &mut sheet_writers.cell_update,
                 ); // Mutable borrow #3 ends here
            }
            else {
                // Immutable access to registry is fine here
                let sheet_data_ref_opt = registry.get_sheet(&current_category_clone, selected_name);

                if sheet_data_ref_opt.is_none() {
                    warn!( "Selected sheet '{:?}/{}' not found in registry for rendering.", current_category_clone, selected_name );
                    ui.vertical_centered(|ui| {
                        // Use cloned values here too
                        ui.label(format!( "Sheet '{:?}/{}' no longer exists...", current_category_clone, selected_name ));
                    });
                    // Need mutable borrow of state again to clear selection
                    if state.selected_sheet_name.as_deref() == Some(selected_name.as_str()) {
                        state.selected_sheet_name = None;
                        state.ai_selected_rows.clear();
                        state.ai_mode = AiModeState::Idle;
                        state.force_filter_recalculation = true;
                    }
                }
                else if let Some(sheet_data_ref) = sheet_data_ref_opt {
                     if let Some(metadata) = &sheet_data_ref.metadata {
                        let num_cols = metadata.columns.len();
                        if metadata.get_filters().len() != num_cols && num_cols > 0 {
                             ui.colored_label( egui::Color32::RED, "Metadata inconsistency detected (cols vs filters)...", );
                             return; // Use early return within the closure
                        }

                        egui::ScrollArea::both()
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                let mut table_builder = TableBuilder::new(ui)
                                    .striped(true)
                                    .resizable(true)
                                    .cell_layout(egui::Layout::left_to_right( egui::Align::Center, ))
                                    .min_scrolled_height(0.0);
                                 if num_cols == 0 {
                                    table_builder = table_builder.column(Column::remainder().resizable(false));
                                } else {
                                    for i in 0..num_cols {
                                        let initial_width = metadata.columns.get(i).and_then(|c| c.width).unwrap_or(120.0);
                                        let col = Column::initial(initial_width).at_least(40.0).resizable(true).clip(true);
                                        table_builder = table_builder.column(col);
                                    }
                                }
                                table_builder
                                    .header(20.0, |header_row| {
                                        // sheet_table_header takes &mut state
                                        sheet_table_header( header_row, metadata, selected_name, &mut state, );
                                    })
                                    .body(|body: TableBody| {
                                        // sheet_table_body takes &mut state
                                        sheet_table_body(
                                            body,
                                            row_height,
                                            &current_category_clone, // Pass clone
                                            selected_name,           // Pass reference to clone
                                            &registry,
                                            &render_cache_res,
                                            sheet_writers.cell_update,
                                            &mut state, // Mutable borrow here is fine as it's nested
                                        );
                                    });
                            });
                    } else {
                         // Use cloned values here
                         warn!( "Metadata object missing for sheet '{:?}/{}' even though sheet data exists.", current_category_clone, selected_name );
                         ui.colored_label( egui::Color32::YELLOW, format!( "Metadata missing for sheet '{:?}/{}'.", current_category_clone, selected_name ), );
                    }
                }
            }
        }
        else {
            // Use cloned value here
             if current_category_clone.is_some() { ui.vertical_centered(|ui| { ui.label("Select a sheet from the category."); }); }
             else { ui.vertical_centered(|ui| { ui.label("Select a category and sheet, or upload JSON."); }); }
        }
    }); // End CentralPanel closure
}