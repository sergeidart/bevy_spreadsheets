// src/ui/elements/editor/table_header.rs
use bevy::prelude::*;
use bevy_egui::egui;
use egui_extras::TableRow;

use crate::sheets::definitions::SheetMetadata; // Added import
use super::state::EditorWindowState; 

/// Renders the header row for the sheet table.
/// Allows clicking headers to open the column options popup.
pub fn sheet_table_header(
    header_row: TableRow, 
    metadata: &SheetMetadata, // Changed to take a reference to SheetMetadata
    sheet_name: &str, 
    state: &mut EditorWindowState, 
) {
    let mut header = header_row; 
    
    let headers = metadata.get_headers();
    let filters = metadata.get_filters();
    let num_cols = headers.len();

    for (c_idx, header_text) in headers.iter().enumerate() {
        header.col(|ui| {
            let display_text = if filters.get(c_idx).cloned().flatten().is_some() {
                format!("{} (Filtered)", header_text)
            } else {
                header_text.clone()
            };

            let header_response = ui.button(display_text);
            if header_response.clicked() {
                state.show_column_options_popup = true;
                state.options_column_target_sheet = sheet_name.to_string(); 
                state.options_column_target_index = c_idx;
                state.column_options_popup_needs_init = true;
                state.options_column_target_category = metadata.category.clone();
            }
            header_response.on_hover_text(format!("Click for options for column '{}'", header_text));
        });
    }
    if num_cols == 0 {
        header.col(|ui| {
            ui.strong("(No Columns)");
        });
    }
}