// src/ui/elements/editor.rs
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use egui_extras::{Column, TableBuilder, TableRow};

use super::popups::*;
use super::top_panel::*;

// Import the specific save function needed
use crate::sheets::systems::io::save::save_single_sheet; // CHANGED IMPORT

use crate::sheets::{
    resources::SheetRegistry, // Import SheetRegistry directly
    events::AddSheetRowRequest,
};
use crate::sheets::events::{
    RequestRenameSheet, RequestDeleteSheet, RequestInitiateFileUpload,
    // SheetOperationFeedback // Import needed if we add direct feedback here
};
use crate::ui::common::ui_for_cell;
use crate::ui::UiFeedbackState;


#[derive(Default)]
pub struct EditorWindowState {
    pub(super) selected_sheet_name: Option<String>,
    pub(super) show_rename_popup: bool,
    pub(super) rename_target: String,
    pub(super) new_name_input: String,
    pub(super) show_delete_confirm_popup: bool,
    pub(super) delete_target: String,
}


pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: Local<EditorWindowState>,
    // Event writers
    mut add_row_event_writer: EventWriter<AddSheetRowRequest>,
    mut rename_event_writer: EventWriter<RequestRenameSheet>,
    mut delete_event_writer: EventWriter<RequestDeleteSheet>,
    mut upload_req_writer: EventWriter<RequestInitiateFileUpload>,
    // *** Still needs ResMut because UI modifies grid data directly ***
    mut registry: ResMut<SheetRegistry>,
    ui_feedback: Res<UiFeedbackState>,
) {
    let ctx = contexts.ctx_mut();

    // --- Show Popups ---
    show_rename_popup(ctx, &mut state, &mut rename_event_writer, &ui_feedback);
    show_delete_confirm_popup(ctx, &mut state, &mut delete_event_writer);


    // --- Main Panel UI ---
    egui::CentralPanel::default().show(ctx, |ui| {
        // --- Show Top Panel ---
        show_top_panel(
            ui,
            &mut state,
            &registry, // Pass immutable borrow for reading names
            &mut add_row_event_writer,
            &mut upload_req_writer,
        );

        // --- Display Feedback Message ---
         if !ui_feedback.last_message.is_empty() {
             let text_color = if ui_feedback.is_error { egui::Color32::RED } else { ui.style().visuals.text_color() };
             ui.colored_label(text_color, &ui_feedback.last_message);
         }
         ui.separator();

         // --- FIX: Check if selected sheet still exists ---
         if let Some(selected_name) = &state.selected_sheet_name {
             if registry.get_sheet(selected_name).is_none() {
                 info!("Selected sheet '{}' no longer exists in registry (likely renamed/deleted), clearing selection.", selected_name);
                 state.selected_sheet_name = None;
             }
         }

        // --- Table View Area ---
         if let Some(sheet_name) = &state.selected_sheet_name {
             // Use a flag to track if any cell changed *this frame* within the table
             let mut table_changed_this_frame = false; // ADDED FLAG

             // Clone sheet name here to avoid borrow checker issues inside closures later
             let sheet_name_clone = sheet_name.clone();

             let maybe_render_info = registry.get_sheet(&sheet_name_clone).and_then(|sheet_data| {
                 sheet_data.metadata.as_ref().map(|metadata| {
                     (
                         metadata.column_headers.clone(),
                         metadata.column_types.clone(),
                         sheet_data.grid.len() // Get row count immutably first
                     )
                 })
             });

             match maybe_render_info {
                 Some((headers, column_types, num_rows)) => {
                     let num_cols = headers.len();
                     if num_cols != column_types.len() {
                         ui.colored_label(egui::Color32::RED, format!("Sheet '{}' metadata error: {} headers vs {} column types.", sheet_name_clone, num_cols, column_types.len()));
                         return; // Early return on critical error
                     }
                     if num_cols == 0 && num_rows > 0 {
                         ui.label("Sheet has data rows but metadata defines 0 columns.");
                     }

                     // --- Table Rendering ---
                     egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
                         let text_style = egui::TextStyle::Body;
                         let row_height = ui.text_style_height(&text_style) + ui.style().spacing.item_spacing.y;
                         let mut table_builder = TableBuilder::new(ui).striped(true).resizable(true).cell_layout(egui::Layout::left_to_right(egui::Align::Center)).min_scrolled_height(0.0);

                         // Setup columns
                         for _ in 0..num_cols {
                            table_builder = table_builder.column(Column::initial(120.0).at_least(40.0).resizable(true).clip(true));
                         }
                         if num_cols == 0 {
                            table_builder = table_builder.column(Column::remainder().resizable(false));
                         }

                         // --- Table Header ---
                         let header_content = |mut header: egui_extras::TableRow| {
                             for header_text in &headers {
                                 header.col(|ui| { ui.strong(header_text); });
                             }
                             if num_cols == 0 { header.col(|ui| {ui.strong("(No Columns)");});}
                         };

                         // --- Table Body (Uses mutable registry access) ---
                         let mut internal_change_flag_for_body = false;
                         let body_content = |mut body: egui_extras::TableBody| {
                             if let Some(sheet_data_mut) = registry.get_sheet_mut(&sheet_name_clone) {
                                 body.rows(row_height, num_rows, |mut row: TableRow| {
                                     let row_index = row.index();
                                     if let Some(current_row_mut) = sheet_data_mut.grid.get_mut(row_index) {
                                         if current_row_mut.len() != num_cols {
                                             current_row_mut.resize(num_cols, String::new());
                                             internal_change_flag_for_body = true;
                                         }

                                         if num_cols == 0 {
                                             row.col(|ui| { ui.label("(No columns defined)"); });
                                             return;
                                         }

                                         for c_idx in 0..num_cols {
                                             row.col(|ui| {
                                                 let col_type = column_types[c_idx];
                                                 let cell_string_mut = &mut current_row_mut[c_idx];
                                                 let cell_id = egui::Id::new("cell")
                                                    .with(&sheet_name_clone)
                                                    .with(row_index)
                                                    .with(c_idx);
                                                 if ui_for_cell(ui, cell_id, cell_string_mut, col_type) {
                                                     internal_change_flag_for_body = true;
                                                 }
                                             });
                                         }
                                     } else {
                                         row.col(|ui| { ui.label("Error: Row data missing"); });
                                     }
                                 });
                             } else {
                                 body.row(row_height, |mut row| {
                                     row.col(|ui| { ui.label("Sheet data became unavailable."); });
                                 });
                             }
                         };

                         // Build the table
                         if num_cols > 0 {
                             table_builder.header(20.0, header_content).body(body_content);
                         } else {
                             table_builder.header(20.0, header_content).body(body_content);
                         }

                         // Update the outer flag
                         if internal_change_flag_for_body {
                             table_changed_this_frame = true;
                         }
                     }); // End ScrollArea

                     // --- Trigger Save IF changes occurred in the table ---
                     if table_changed_this_frame {
                         info!("UI table change detected, triggering immediate save for '{}'.", sheet_name_clone);
                         // Pass immutable borrow and sheet name to the save function
                         save_single_sheet(&registry, &sheet_name_clone); // MODIFIED CALL
                     }

                 } // End Some((headers, ...))
                 None => {
                     // Metadata missing or sheet disappeared handling
                      if registry.get_sheet(&sheet_name_clone).is_some() { // Use cloned name
                         ui.colored_label(
                             egui::Color32::YELLOW,
                             format!("Metadata missing for sheet '{}'. Cannot render table.", sheet_name_clone)
                         );
                     } else {
                         ui.vertical_centered(|ui| {
                             ui.label("Selected sheet is no longer available.");
                             ui.label("Please select another sheet.");
                         });
                     }
                 } // End None case for maybe_render_info
             } // End match maybe_render_info
         } else {
             // No sheet selected
             ui.vertical_centered(|ui| {
                 ui.label("Select a sheet from the dropdown or upload a JSON file.");
             });
         }
    }); // End CentralPanel
}