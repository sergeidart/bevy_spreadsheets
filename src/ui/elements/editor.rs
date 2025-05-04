// src/ui/elements/editor.rs
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use egui_extras::{Column, TableBuilder, TableRow};

use super::popups::*;
use super::top_panel::*;

// Import the specific save function needed
use crate::sheets::systems::io::save::save_single_sheet;

use crate::sheets::{
    resources::SheetRegistry,
    events::{
        AddSheetRowRequest,
        RequestRenameSheet,
        RequestDeleteSheet,
        RequestInitiateFileUpload,
        RequestUpdateColumnName, // Added Event
        // SheetOperationFeedback // Import needed if we add direct feedback here
    },
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
    // Added State for Column Rename
    pub(super) show_column_rename_popup: bool,
    pub(super) rename_column_target_sheet: String,
    pub(super) rename_column_target_index: usize,
    pub(super) rename_column_input: String,
}


pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: Local<EditorWindowState>,
    // Event writers
    mut add_row_event_writer: EventWriter<AddSheetRowRequest>,
    mut rename_event_writer: EventWriter<RequestRenameSheet>,
    mut delete_event_writer: EventWriter<RequestDeleteSheet>,
    mut upload_req_writer: EventWriter<RequestInitiateFileUpload>,
    mut column_rename_writer: EventWriter<RequestUpdateColumnName>, // Added Writer
    // *** Still needs ResMut because UI modifies grid data directly ***
    mut registry: ResMut<SheetRegistry>,
    ui_feedback: Res<UiFeedbackState>,
) {
    let ctx = contexts.ctx_mut();

    // --- Main Panel UI ---
    egui::CentralPanel::default().show(ctx, |ui| {
        // --- Show Top Panel ---
        // This takes immutable borrow of state indirectly via registry and directly for selected sheet
        show_top_panel(
            ui,
            &mut state, // Pass mutable state for selection change
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

         // --- FIX: Check if selected sheet still exists (before table rendering) ---
         // This requires reading state immutably
         let mut clear_selection = false;
         if let Some(selected_name) = &state.selected_sheet_name {
             if registry.get_sheet(selected_name).is_none() {
                 info!("Selected sheet '{}' no longer exists in registry (likely renamed/deleted), queueing selection clear.", selected_name);
                 clear_selection = true;
             }
         }
         // Apply clearing outside the immutable borrow scope if needed
         if clear_selection {
              state.selected_sheet_name = None;
              // Also clear column rename state if it was for the removed sheet
              // Check before clearing state.rename_column_target_sheet potentially
              // This part might need refinement depending on exact borrow needs vs clearing logic
              state.show_column_rename_popup = false;
              state.rename_column_target_sheet.clear();
              state.rename_column_target_index = 0;
              state.rename_column_input.clear();
         }


        // --- Table View Area ---
        // This section reads state immutably (selected_sheet_name)
        // and later borrows registry mutably within the TableBody closure
         if let Some(sheet_name) = &state.selected_sheet_name {
             let mut table_changed_this_frame = false;
             let sheet_name_clone = sheet_name.clone(); // Clone for use in closures

             // Get metadata immutably first
             let maybe_render_info = registry.get_sheet(&sheet_name_clone).and_then(|sheet_data| {
                 sheet_data.metadata.as_ref().map(|metadata| {
                     (
                         metadata.column_headers.clone(),
                         metadata.column_types.clone(),
                         sheet_data.grid.len()
                     )
                 })
             });

             match maybe_render_info {
                 Some((headers, column_types, num_rows)) => {
                     let num_cols = headers.len();
                     if num_cols != column_types.len() { /* ... error label ... */ return; }
                     if num_cols == 0 && num_rows > 0 { /* ... info label ... */ }

                     // --- Table Rendering ---
                     egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
                         let text_style = egui::TextStyle::Body;
                         let row_height = ui.text_style_height(&text_style) + ui.style().spacing.item_spacing.y;
                         let mut table_builder = TableBuilder::new(ui).striped(true).resizable(true).cell_layout(egui::Layout::left_to_right(egui::Align::Center)).min_scrolled_height(0.0);

                         for _ in 0..num_cols { table_builder = table_builder.column(Column::initial(120.0).at_least(40.0).resizable(true).clip(true)); }
                         if num_cols == 0 { table_builder = table_builder.column(Column::remainder().resizable(false)); }

                         // --- Table Header (Modified - needs mutable state for popup trigger) ---
                         let header_content = |mut header: egui_extras::TableRow| {
                             for (c_idx, header_text) in headers.iter().enumerate() {
                                 header.col(|ui| {
                                     let header_response = ui.button(header_text);
                                     if header_response.clicked() {
                                         // Defer state mutation if possible, or ensure borrow ends quickly.
                                         // Direct mutation here might still conflict if borrow checker sees it overlapping.
                                         // Let's try direct mutation for now:
                                         state.show_column_rename_popup = true;
                                         state.rename_column_target_sheet = sheet_name_clone.clone();
                                         state.rename_column_target_index = c_idx;
                                         state.rename_column_input = header_text.clone();
                                     }
                                     header_response.on_hover_text(format!("Click to rename column '{}'", header_text));
                                 });
                             }
                             if num_cols == 0 { header.col(|ui| {ui.strong("(No Columns)");});}
                         };

                         // --- Table Body (Borrows registry mutably inside) ---
                         let mut internal_change_flag_for_body = false;
                         let body_content = |mut body: egui_extras::TableBody| {
                             // This gets registry mutably, separate from the outer state borrow
                             if let Some(sheet_data_mut) = registry.get_sheet_mut(&sheet_name_clone) {
                                 body.rows(row_height, num_rows, |mut row: TableRow| {
                                     let row_index = row.index();
                                     if row_index < sheet_data_mut.grid.len() {
                                         if sheet_data_mut.grid[row_index].len() != num_cols {
                                             // This mutation happens within the registry borrow scope
                                             sheet_data_mut.grid[row_index].resize(num_cols, String::new());
                                             internal_change_flag_for_body = true;
                                         }

                                         if num_cols == 0 { row.col(|ui| { ui.label("(No columns defined)"); }); return; }

                                         let current_row_mut = &mut sheet_data_mut.grid[row_index];
                                         for c_idx in 0..num_cols {
                                             row.col(|ui| {
                                                 if c_idx < column_types.len() && c_idx < current_row_mut.len() {
                                                    let col_type = column_types[c_idx];
                                                    let cell_string_mut = &mut current_row_mut[c_idx];
                                                    let cell_id = egui::Id::new("cell").with(&sheet_name_clone).with(row_index).with(c_idx);
                                                    // ui_for_cell takes &mut String, part of the registry borrow
                                                    if ui_for_cell(ui, cell_id, cell_string_mut, col_type) {
                                                        internal_change_flag_for_body = true;
                                                    }
                                                 } else { ui.label("Error: Index invalid"); }
                                             });
                                         } // End column loop
                                     } else { row.col(|ui| { ui.label("Error: Row index invalid"); }); }
                                 }); // End body.rows
                             } else { body.row(row_height, |mut row| { row.col(|ui| { ui.label("Sheet data unavailable."); }); }); }
                         }; // End body_content closure definition

                         // Build the table
                         if num_cols > 0 {
                             table_builder.header(20.0, header_content).body(body_content);
                         } else {
                             table_builder.header(20.0, header_content).body(body_content);
                         }

                         if internal_change_flag_for_body {
                             table_changed_this_frame = true;
                         }
                     }); // End ScrollArea

                     // --- Trigger Save IF changes occurred in the table ---
                     // This needs immutable registry borrow
                     if table_changed_this_frame {
                         info!("UI table change detected, triggering immediate save for '{}'.", sheet_name_clone);
                         save_single_sheet(&registry, &sheet_name_clone);
                     }

                 } // End Some((headers, ...))
                 None => {
                     // Metadata missing or sheet disappeared handling
                      if registry.get_sheet(&sheet_name_clone).is_some() {
                         ui.colored_label(egui::Color32::YELLOW, format!("Metadata missing for sheet '{}'.", sheet_name_clone));
                     } else {
                         ui.vertical_centered(|ui| { ui.label("Selected sheet not available."); ui.label("Select another sheet."); });
                     }
                 } // End None case for maybe_render_info
             } // End match maybe_render_info
         } else {
             // No sheet selected
             ui.vertical_centered(|ui| { ui.label("Select a sheet or upload JSON."); });
         }
         // Panel UI definition ends here
    }); // End CentralPanel::show

    // --- Show Popups (Moved to the end) ---
    // These take mutable borrows of state, which should now be fine
    // as the borrows for the CentralPanel UI elements have ended.
    show_rename_popup(ctx, &mut state, &mut rename_event_writer, &ui_feedback);
    show_delete_confirm_popup(ctx, &mut state, &mut delete_event_writer);
    show_column_rename_popup(ctx, &mut state, &mut column_rename_writer);

} // End generic_sheet_editor_ui function