// src/ui/elements/editor.rs
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use egui_extras::{Column, TableBuilder, TableRow}; // Use TableBuilder

// Use types from the sheets module *within this crate*
use crate::sheets::{
    SheetRegistry,
    SheetMetadata, // Needed for inspection
    ColumnDataType, // Needed for cell rendering
    RequestSaveSheets, AddSheetRowRequest, // Events to send
};

// Use common UI helpers *within this crate*
use crate::ui::common::ui_for_cell;

// --- Local UI State ---
#[derive(Default)]
pub struct EditorWindowState {
    selected_sheet_name: Option<&'static str>,
    // Simplified state for standalone app
}

// The Generic Sheet Editor UI System for the standalone app.
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts, // Standard Egui system parameter
    mut state: Local<EditorWindowState>,
    mut save_event_writer: EventWriter<RequestSaveSheets>,
    mut add_row_event_writer: EventWriter<AddSheetRowRequest>,
    mut registry_mut: ResMut<SheetRegistry>, // Still need mutable access
){
    let ctx = contexts.ctx_mut(); // Get context

    // Use CentralPanel to fill the main window
    egui::CentralPanel::default().show(ctx, |ui| {
        // --- Top Controls Row ---
         ui.horizontal(|ui| {
             // Sheet Selector (Reads registry)
             ui.label("Select Sheet:");
             let sheet_names = registry_mut.get_sheet_names(); // Get names from registry resource
             egui::ComboBox::from_id_source("sheet_selector_grid")
                 .selected_text(state.selected_sheet_name.unwrap_or("--Select--"))
                 .show_ui(ui, |ui| {
                     // Provide Option<&'static str> for state update
                     ui.selectable_value(&mut state.selected_sheet_name, None, "--Select--");
                     for name in sheet_names {
                         ui.selectable_value(&mut state.selected_sheet_name, Some(*name), *name);
                     }
                 });

             // Spacer + Right-aligned buttons
             ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                 // Save Button (Sends event)
                 if ui.button("💾 Save All Sheets").on_hover_text("Saves all sheets to ./data_sheets/").clicked() {
                     save_event_writer.send(RequestSaveSheets);
                 }
                 // Add Row Button (Sends event) - Enabled only if a sheet is selected
                 let add_row_enabled = state.selected_sheet_name.is_some();
                 if ui.add_enabled(add_row_enabled, egui::Button::new("➕ Add Row")).clicked() {
                      if let Some(sheet_name) = state.selected_sheet_name {
                         add_row_event_writer.send(AddSheetRowRequest { sheet_name });
                      }
                 }
                 // TODO: Add Remove Row button later if needed (requires row selection state)
             });
         }); // End Top Controls Row
         ui.separator();

         // --- Table View Area ---
         if let Some(sheet_name) = state.selected_sheet_name {
            // Need mutable access to the grid to modify it via ui_for_cell
            if let Some(sheet_data_mut) = registry_mut.get_sheet_mut(sheet_name) {
                // Clone metadata needed for rendering to avoid double borrow issues
                if let Some(metadata) = &sheet_data_mut.metadata.clone() {
                    let headers = metadata.column_headers;
                    let column_types = metadata.column_types;
                    let num_cols = headers.len();

                    // Basic validation
                    if num_cols == 0 || num_cols != column_types.len() {
                        ui.colored_label(egui::Color32::RED, format!("Sheet '{}' metadata error (cols: {}, types: {}).", sheet_name, num_cols, column_types.len()));
                        return; // Stop rendering this sheet if metadata is bad
                    }

                    let num_rows = sheet_data_mut.grid.len(); // Get current number of rows

                    // Use ScrollArea to handle large tables
                    egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
                        // --- TableBuilder Logic ---
                        let text_style = egui::TextStyle::Body;
                        let row_height = ui.text_style_height(&text_style); // Calculate default row height

                        let mut table = TableBuilder::new(ui)
                            .striped(true) // Alternate row background colors
                            .resizable(true) // Allow column resizing
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                            .min_scrolled_height(0.0); // Take up available height

                        // Define columns based on metadata
                        for _ in 0..num_cols {
                            // Adjust initial width/min_width as needed
                            table = table.column(Column::initial(120.0).at_least(40.0).resizable(true).clip(true));
                        }

                        // Header row
                        table.header(20.0, |mut header_row| {
                            for header in headers {
                                header_row.col(|ui| {
                                    ui.strong(*header); // Display header text bolded
                                });
                            }
                        })
                        // Body rows
                        .body(|body| {
                            body.rows(row_height, num_rows, |mut row: TableRow| {
                                let row_index = row.index();
                                // Ensure row exists and has correct length before processing cells
                                if let Some(current_row_mut) = sheet_data_mut.grid.get_mut(row_index) {
                                    // Handle potential mismatch if columns were added/removed without data migration
                                    if current_row_mut.len() != num_cols {
                                        // Pad with empty strings if too short, or truncate if too long
                                        current_row_mut.resize(num_cols, String::new());
                                    }

                                    // Render each cell in the row
                                    for c_idx in 0..num_cols {
                                        row.col(|ui| {
                                            let col_type = column_types[c_idx];
                                            // Unique ID for each cell widget state
                                            let cell_id = egui::Id::new((sheet_name, row_index, c_idx));
                                            // Pass mutable reference to the cell string to ui_for_cell
                                            let cell_string_mut = &mut current_row_mut[c_idx];
                                            // ui_for_cell renders the appropriate widget and modifies cell_string_mut directly
                                            let _changed = ui_for_cell(ui, cell_id, cell_string_mut, col_type);
                                            // Changed flag not strictly needed here as we modify ResMut directly
                                        });
                                    }
                                } else {
                                     // Should ideally not happen if num_rows is correct, but handle defensively
                                     row.col(|ui| { ui.label("..."); });
                                }
                            }); // End rows loop
                        }); // End table body
                        // --- End TableBuilder Logic ---
                    }); // End ScrollArea

                } else {
                    // This case means metadata wasn't registered correctly
                    ui.colored_label(egui::Color32::RED, format!("Metadata missing for sheet '{}'. Cannot render.", sheet_name));
                }
            } else {
                // This case means the sheet name exists but getting mutable access failed (less likely)
                 ui.label("Waiting for sheet data access...");
            }

         } else {
             // No sheet selected
             ui.vertical_centered(|ui| {
                 ui.label("Select a sheet from the dropdown above to view or edit.");
             });
         }
    }); // End CentralPanel
}