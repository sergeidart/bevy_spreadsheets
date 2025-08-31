// src/ui/elements/editor/editor_sheet_display.rs
use bevy::prelude::*;
use bevy_egui::egui; // EguiContexts might not be needed here if ctx is passed
use egui_extras::{Column, TableBody, TableBuilder};
use crate::sheets::{
    resources::{SheetRegistry, SheetRenderCache},
    events::{RequestReorderColumn, UpdateCellEvent, OpenStructureViewEvent},
};
use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::elements::editor::table_header::sheet_table_header;
use crate::ui::elements::editor::table_body::sheet_table_body;
// No longer need to import SheetEventWriters as a whole struct here

#[allow(clippy::too_many_arguments)]
pub(super) fn show_sheet_table(
    ui: &mut egui::Ui,
    ctx: &egui::Context, 
    row_height: f32,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    render_cache: &SheetRenderCache,
    // MODIFIED: Accept individual EventWriters directly by value
    reorder_column_writer: EventWriter<RequestReorderColumn>,
    cell_update_writer: EventWriter<UpdateCellEvent>,
    mut open_structure_writer: EventWriter<OpenStructureViewEvent>,
) {
    // If a virtual structure sheet is active, temporarily override selected sheet for rendering
    let backup_sheet = state.selected_sheet_name.clone();
    if let Some(vctx) = state.virtual_structure_stack.last() {
        if let Some(vsheet) = registry.get_sheet(&state.selected_category, &vctx.virtual_sheet_name) {
            if vsheet.metadata.is_some() {
                state.selected_sheet_name = Some(vctx.virtual_sheet_name.clone());
            }
        }
    }
    if let Some(selected_name) = &state.selected_sheet_name.clone() { 
        let current_category_clone = state.selected_category.clone(); 

        let sheet_data_ref_opt = registry.get_sheet(&current_category_clone, selected_name);

        if sheet_data_ref_opt.is_none() {
            warn!("Selected sheet '{:?}/{}' not found in registry for rendering.", current_category_clone, selected_name);
            ui.vertical_centered(|ui| { ui.label(format!("Sheet '{:?}/{}' no longer exists...", current_category_clone, selected_name)); });
            if state.selected_sheet_name.as_deref() == Some(selected_name.as_str()) {
                state.selected_sheet_name = None;
                state.reset_interaction_modes_and_selections();
                state.force_filter_recalculation = true;
            }
            return;
        }

        if let Some(sheet_data_ref) = sheet_data_ref_opt {
             if let Some(metadata) = &sheet_data_ref.metadata {
                // Notice for standard sheets only (structure view handled earlier)
                let num_cols = metadata.columns.len();
                if metadata.get_filters().len() != num_cols && num_cols > 0 {
                     error!("Metadata inconsistency detected (cols vs filters) for sheet '{:?}/{}'. Revalidation might be needed.", current_category_clone, selected_name);
                     ui.colored_label(egui::Color32::RED, "Metadata inconsistency detected...");
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
                            const DEFAULT_COL_WIDTH: f32 = 120.0; // width field deprecated
                            for _i in 0..num_cols {
                                let col = Column::initial(DEFAULT_COL_WIDTH).at_least(40.0).resizable(true).clip(true);
                                table_builder = table_builder.column(col);
                            }
                       }
                       if let Some(row_idx) = state.scroll_to_row_index {
                            if num_cols > 0 {
                                table_builder = table_builder.scroll_to_row(row_idx, Some(egui::Align::TOP));
                            }
                            state.scroll_to_row_index = None;
                       }
                       table_builder
                           .header(20.0, |mut header_row| {
                               // Pass the received EventWriter by value (it's already a copy)
                               sheet_table_header(&mut header_row, ctx, metadata, selected_name, state, reorder_column_writer);
                            })
                           .body(|body: TableBody| {
                               // Pass the received EventWriter by value
                               sheet_table_body(body, row_height, &current_category_clone, selected_name, registry, render_cache, cell_update_writer, state, &mut open_structure_writer);
                           });
                    });
            } else {
                 warn!("Metadata object missing for sheet '{:?}/{}' even though sheet data exists.", current_category_clone, selected_name);
                 ui.colored_label(egui::Color32::YELLOW, format!("Metadata missing for sheet '{:?}/{}'.", current_category_clone, selected_name));
            }
        }
    } else {
         if state.selected_category.is_some() {
            ui.vertical_centered(|ui| { ui.label("Select a sheet from the category, or upload a new one."); });
         }
         else {
            ui.vertical_centered(|ui| { ui.label("Select a category and sheet, or upload JSON."); });
         }
    }
    // Restore selection if we temporarily switched to virtual
    if let Some(vctx) = state.virtual_structure_stack.last() {
        if let Some(orig_sheet) = &backup_sheet { if *orig_sheet != vctx.virtual_sheet_name { state.selected_sheet_name = backup_sheet; } }
    } else {
        // No virtual sheet active, ensure selection remains original
        state.selected_sheet_name = backup_sheet;
    }
}