// src/ui/elements/top_panel.rs
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::resources::SheetRegistry;
use crate::sheets::events::{
    AddSheetRowRequest, // RequestSaveSheets removed
    RequestRenameSheet, RequestDeleteSheet,
    RequestInitiateFileUpload,
};
use super::editor::EditorWindowState;

/// Renders the top control panel with sheet selector and action buttons.
pub fn show_top_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    // save_event_writer: &mut EventWriter<RequestSaveSheets>, // Removed
    add_row_event_writer: &mut EventWriter<AddSheetRowRequest>,
    upload_req_writer: &mut EventWriter<RequestInitiateFileUpload>,
) {
    ui.horizontal(|ui| {
        // --- Sheet Selector ---
        ui.label("Select Sheet:");
        let sheet_names = registry.get_sheet_names().clone();
        let selected_text = state.selected_sheet_name.as_deref().unwrap_or("--Select--");

        egui::ComboBox::from_id_source("sheet_selector_grid")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.selected_sheet_name, None, "--Select--");
                for name in sheet_names {
                    ui.selectable_value(&mut state.selected_sheet_name, Some(name.clone()), &name);
                }
            });

        let selected_sheet_name_cache = state.selected_sheet_name.clone();
        let is_sheet_selected = selected_sheet_name_cache.is_some();

        // --- Buttons for Selected Sheet ---
        if ui
            .add_enabled(is_sheet_selected, egui::Button::new("✏ Rename"))
            .clicked()
        {
            if let Some(ref name_to_rename) = selected_sheet_name_cache {
                state.rename_target = name_to_rename.clone();
                state.new_name_input = state.rename_target.clone();
                state.show_rename_popup = true;
            }
        }
        if ui
            .add_enabled(is_sheet_selected, egui::Button::new("🗑 Delete"))
            .clicked()
        {
            if let Some(ref name_to_delete) = selected_sheet_name_cache {
                state.delete_target = name_to_delete.clone();
                state.show_delete_confirm_popup = true;
            }
        }

        // --- Global Buttons ---
        if ui
            .button("⬆ Upload JSON")
            .on_hover_text("Upload a JSON file")
            .clicked()
        {
            upload_req_writer.send(RequestInitiateFileUpload);
        }

        // Spacer + Right-aligned buttons
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // --- Save All Button Removed ---
            // if ui.button("💾 Save All")... removed

            // Add Row Button (remains)
            if ui
                .add_enabled(is_sheet_selected, egui::Button::new("➕ Add Row"))
                .clicked()
            {
                if let Some(sheet_name) = &state.selected_sheet_name {
                    add_row_event_writer
                        .send(AddSheetRowRequest { sheet_name: sheet_name.clone() });
                }
            }
        });
    }); // End Top Controls Horizontal layout
}