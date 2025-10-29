// src/ui/widgets/context_menu_helpers.rs

use bevy::prelude::*;
use bevy_egui::egui;
use crate::sheets::{
    events::{RequestCopyCell, RequestPasteCell},
    resources::ClipboardBuffer,
};

/// Adds a standard cell context menu with Copy, Paste, and Clear operations.
///
/// This helper provides consistent context menu behavior across all cell types.
/// Returns the original response to maintain the call chain.
///
/// # Arguments
/// * `response` - The egui Response to attach the context menu to
/// * `category` - Optional category for the cell
/// * `sheet_name` - Name of the sheet containing the cell
/// * `row_index` - Row index of the cell
/// * `col_index` - Column index of the cell
/// * `copy_events` - Event writer for copy operations
/// * `paste_events` - Event writer for paste operations
/// * `clipboard_buffer` - Clipboard buffer resource to check if paste is available
/// * `temp_new_value` - Mutable reference to set new value on clear
pub fn add_cell_context_menu(
    response: egui::Response,
    category: &Option<String>,
    sheet_name: &str,
    row_index: usize,
    col_index: usize,
    copy_events: &mut EventWriter<RequestCopyCell>,
    paste_events: &mut EventWriter<RequestPasteCell>,
    clipboard_buffer: &ClipboardBuffer,
    temp_new_value: &mut Option<String>,
) -> egui::Response {
    let _ = response.context_menu(|menu_ui| {
        if menu_ui.button("📋 Copy").clicked() {
            copy_events.write(RequestCopyCell {
                category: category.clone(),
                sheet_name: sheet_name.to_string(),
                row_index,
                col_index,
            });
            menu_ui.close_menu();
        }
        let has_clipboard_data = clipboard_buffer.cell_value.is_some();
        if menu_ui
            .add_enabled(has_clipboard_data, egui::Button::new("📄 Paste"))
            .clicked()
        {
            paste_events.write(RequestPasteCell {
                category: category.clone(),
                sheet_name: sheet_name.to_string(),
                row_index,
                col_index,
            });
            menu_ui.close_menu();
        }
        if menu_ui.button("🗑 Clear").clicked() {
            *temp_new_value = Some(String::new());
            menu_ui.close_menu();
        }
    });
    response
}
