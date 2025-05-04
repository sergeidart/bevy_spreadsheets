// src/ui/elements/editor.rs
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use egui_extras::{Column, TableBuilder, TableRow}; // Added TableRow explicitly
use std::{fs, path::PathBuf}; // For reading file path

// Import the function from its new location in io.rs
use crate::sheets::systems::io::load::load_and_parse_json_sheet;

// Use types from the sheets module
use crate::sheets::{
    SheetRegistry,
    SheetMetadata,
    ColumnDataType,
    RequestSaveSheets, AddSheetRowRequest, // Keep these direct imports
};
use crate::sheets::events::JsonSheetUploaded;
use crate::ui::common::ui_for_cell;

#[derive(Default)]
pub struct EditorWindowState {
    selected_sheet_name: Option<String>,
    upload_status_message: String, // To show feedback to user
}

// The Generic Sheet Editor UI System
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: Local<EditorWindowState>, // Use Local for per-system state
    mut save_event_writer: EventWriter<RequestSaveSheets>,
    mut add_row_event_writer: EventWriter<AddSheetRowRequest>,
    mut upload_event_writer: EventWriter<JsonSheetUploaded>,
    mut registry: ResMut<SheetRegistry>,
) {
    let ctx = contexts.ctx_mut();

    egui::CentralPanel::default().show(ctx, |ui| {
        // --- Top Controls Row ---
        ui.horizontal(|ui| {
            // Sheet Selector
            ui.label("Select Sheet:");
            // Clone the list of names for the combo box UI
            let sheet_names = registry.get_sheet_names().clone();
            let selected_text = state.selected_sheet_name.as_deref().unwrap_or("--Select--");

            // Use combo box with owned String state
            egui::ComboBox::from_id_source("sheet_selector_grid")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.selected_sheet_name, None, "--Select--");
                    for name in sheet_names {
                        // Pass Option<String> for state update
                        ui.selectable_value(&mut state.selected_sheet_name, Some(name.clone()), &name);
                    }
                });

             // --- Upload Button ---
             if ui.button("⬆ Upload JSON").on_hover_text("Upload a JSON file (expected: array of arrays of strings)").clicked() {
                state.upload_status_message.clear(); // Clear previous message
                // Use async dialog picker if in async context, otherwise blocking
                let picked_file = rfd::FileDialog::new()
                    .add_filter("JSON files", &["json"])
                    .pick_file(); // This is blocking

                match picked_file {
                    Some(path) => {
                        state.upload_status_message = format!("Processing '{}'...", path.display());
                        // Use the function imported from io.rs
                        match load_and_parse_json_sheet(&path) {
                             Ok((grid, filename)) => {
                                // Use filename stem as default sheet name
                                let desired_name = path.file_stem()
                                                     .map(|s| s.to_string_lossy().into_owned())
                                                     .unwrap_or_else(|| filename.trim_end_matches(".json").to_string()); // Fallback

                                // Prevent empty sheet names from upload attempt
                                if desired_name.is_empty() {
                                     error!("Upload failed: Could not determine sheet name from filename '{}'.", path.display());
                                     state.upload_status_message = format!("Error uploading '{}': Could not derive sheet name.", path.display());
                                } else {
                                    // Send event with parsed data
                                    upload_event_writer.send(JsonSheetUploaded {
                                        desired_sheet_name: desired_name.clone(),
                                        original_filename: filename, // Pass the actual filename
                                        grid_data: grid,
                                    });
                                    // Automatically select the newly uploaded sheet
                                    state.selected_sheet_name = Some(desired_name);
                                    state.upload_status_message = format!("Successfully requested upload for '{}'.", path.display());
                                }
                             }
                             Err(e) => {
                                 error!("Upload failed: {}", e);
                                 state.upload_status_message = format!("Error uploading '{}': {}", path.display(), e);
                             }
                         }
                    }
                    None => {
                         state.upload_status_message = "File selection cancelled.".to_string();
                    }
                }
             }

            // Spacer + Right-aligned buttons
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Save Button
                if ui.button("💾 Save All").on_hover_text("Saves all sheets to ./data_sheets/").clicked() {
                    save_event_writer.send(RequestSaveSheets);
                }
                // Add Row Button
                let add_row_enabled = state.selected_sheet_name.is_some();
                if ui.add_enabled(add_row_enabled, egui::Button::new("➕ Add Row")).clicked() {
                    // Clone the name if needed for the event
                    if let Some(sheet_name) = &state.selected_sheet_name {
                        add_row_event_writer.send(AddSheetRowRequest { sheet_name: sheet_name.clone() });
                    }
                }
                // TODO: Add Remove Row button later if needed
            });
        }); // End Top Controls Row

        if !state.upload_status_message.is_empty() {
             // Use a different color for errors vs success
            let text_color = if state.upload_status_message.starts_with("Error") {
                egui::Color32::RED
            } else {
                ui.style().visuals.text_color()
            };
            ui.colored_label(text_color, &state.upload_status_message);
        }
        ui.separator();


        // --- Table View Area ---
        if let Some(sheet_name) = &state.selected_sheet_name {
             // Get mutable access using the String name
            if let Some(sheet_data) = registry.get_sheet_mut(sheet_name) { // Borrow sheet_data mutably

                // Use a separate scope to borrow metadata immutably/cloned
                // and then release it before the mutable borrow for the grid body.
                let (headers, column_types, num_cols, num_rows) = {
                     if let Some(metadata) = sheet_data.metadata.clone() { // Clone metadata
                        let headers = metadata.column_headers; // headers is Vec<String>
                        let column_types = metadata.column_types; // column_types is Vec<ColumnDataType>
                        let num_cols = headers.len();

                        // Basic validation
                        if num_cols != column_types.len() {
                            ui.colored_label(egui::Color32::RED, format!("Sheet '{}' metadata error: {} headers, {} types.", sheet_name, num_cols, column_types.len()));
                            return; // Stop rendering this sheet if metadata is bad
                        }
                        // Handle case with 0 columns gracefully
                        if num_cols == 0 && !sheet_data.grid.is_empty() {
                             ui.label("Sheet has data rows but metadata defines 0 columns.");
                        }
                        let num_rows = sheet_data.grid.len(); // Get num_rows based on current grid
                        (headers, column_types, num_cols, num_rows)
                    } else {
                        // Metadata missing or being generated
                        ui.colored_label(egui::Color32::YELLOW, format!("Metadata missing or being generated for sheet '{}'.", sheet_name));
                        return; // Cannot render table without metadata
                    }
                }; // End immutable/clone borrow scope for metadata

                // Use ScrollArea to handle potentially large tables
                egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
                    // --- TableBuilder Logic ---
                    let text_style = egui::TextStyle::Body;
                    let row_height = ui.text_style_height(&text_style) + ui.style().spacing.item_spacing.y;

                    let mut table_builder = TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .min_scrolled_height(0.0);

                    // Define columns based on metadata (even if 0)
                    for _ in 0..num_cols {
                        table_builder = table_builder.column(Column::initial(120.0).at_least(40.0).resizable(true).clip(true));
                    }
                     // If there are no columns, add a placeholder column to prevent TableBuilder panic
                     if num_cols == 0 {
                         table_builder = table_builder.column(Column::remainder());
                     }

                    // --- Corrected Chaining ---
                    // Closure for the body logic to avoid duplication
                    let body_logic = |body: egui_extras::TableBody| {
                        body.rows(row_height, num_rows, |mut row: TableRow| {
                             let row_index = row.index();
                             // Access sheet_data.grid mutably HERE, inside the body closure
                             if let Some(current_row_mut) = sheet_data.grid.get_mut(row_index) {
                                 // Ensure row has correct number of columns (pad/truncate)
                                 if current_row_mut.len() != num_cols {
                                     current_row_mut.resize(num_cols, String::new());
                                 }
                                 // If there are no columns, display something informative in the row
                                 if num_cols == 0 {
                                      row.col(|ui| { ui.label("(No columns defined)"); });
                                      return; // Skip cell processing for this row
                                  }
                                 // Render each cell in the row
                                 for c_idx in 0..num_cols {
                                     row.col(|ui| {
                                         // These unwraps should be safe due to resize/checks above
                                         let col_type = column_types[c_idx];
                                         let cell_string_mut = &mut current_row_mut[c_idx];
                                         let cell_id = egui::Id::new("cell").with(sheet_name).with(row_index).with(c_idx);
                                         ui_for_cell(ui, cell_id, cell_string_mut, col_type);
                                     });
                                 }
                             } else {
                                 row.col(|ui| { ui.label("..."); }); // Defensive
                             }
                         }); // End rows loop
                    };

                    // Start the header/body chain from the configured builder
                    if num_cols > 0 {
                        // If header exists, chain .body() onto .header()
                        table_builder
                            .header(20.0, |mut header_row| {
                                for header in &headers { // Borrow headers here
                                    header_row.col(|ui| {
                                        ui.strong(header);
                                    });
                                }
                            })
                            .body(body_logic); // Pass the body logic closure
                    } else {
                        // If no header, call .body() directly on the builder
                        table_builder.body(body_logic); // Pass the body logic closure
                    };
                    // --- End Corrected Chaining ---
                }); // End ScrollArea

            } else {
                // This case means the sheet name exists but getting mutable access failed
                 ui.label("Waiting for sheet data access...");
            }
        } else {
            // No sheet selected
            ui.vertical_centered(|ui| {
                ui.label("Select a sheet from the dropdown or upload a JSON file.");
            });
        }
    }); // End CentralPanel
}