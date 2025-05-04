// src/ui/elements/editor/table_header.rs
use bevy::prelude::*;
use bevy_egui::egui;
use egui_extras::TableRow;

use super::state::EditorWindowState; // Need mutable state to trigger popup

/// Renders the header row for the sheet table.
/// Allows clicking headers to open the column options popup.
pub fn sheet_table_header(
    header: TableRow, // egui_extras header row
    headers: &[String],
    filters: &[Option<String>],
    sheet_name: &str,
    state: &mut EditorWindowState, // Mutable state to set popup flags
) {
    let mut header = header; // Make mutable for iteration
    let num_cols = headers.len();

    for (c_idx, header_text) in headers.iter().enumerate() {
        header.col(|ui| {
            // Add indicator if filter is active
            let display_text = if filters.get(c_idx).cloned().flatten().is_some() {
                format!("{} (Filtered)", header_text)
            } else {
                header_text.clone()
            };

            let header_response = ui.button(display_text);
            // Trigger the options popup on click
            if header_response.clicked() {
                state.show_column_options_popup = true;
                state.options_column_target_sheet = sheet_name.to_string();
                state.options_column_target_index = c_idx;
                // Mark that popup needs init
                state.column_options_popup_needs_init = true;
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