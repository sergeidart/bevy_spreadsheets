// src/ui/elements/editor/main_editor.rs
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use egui_extras::{Column, TableBuilder};

// Import components from sibling modules
use super::state::EditorWindowState;
use super::table_header::sheet_table_header;
use super::table_body::sheet_table_body;

// Import other necessary UI elements and systems
use crate::ui::elements::top_panel::show_top_panel;
use crate::ui::elements::popups::{
    show_column_options_popup, show_delete_confirm_popup, show_rename_popup,
};
use crate::sheets::{
    resources::SheetRegistry,
    events::{
        AddSheetRowRequest, RequestRenameSheet, RequestDeleteSheet,
        RequestInitiateFileUpload, RequestUpdateColumnName,
        RequestUpdateColumnValidator,
        UpdateCellEvent,
    },
};
use crate::ui::UiFeedbackState;


/// The main UI assembly function for the sheet editor.
/// Orchestrates the top panel, feedback, table, and popups.
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: Local<EditorWindowState>, // Keep as Local state
    // Event writers
    mut add_row_event_writer: EventWriter<AddSheetRowRequest>,
    mut rename_event_writer: EventWriter<RequestRenameSheet>,
    mut delete_event_writer: EventWriter<RequestDeleteSheet>,
    mut upload_req_writer: EventWriter<RequestInitiateFileUpload>,
    mut column_rename_writer: EventWriter<RequestUpdateColumnName>,
    mut column_validator_writer: EventWriter<RequestUpdateColumnValidator>,
    mut cell_update_writer: EventWriter<UpdateCellEvent>,
    // Resources
    mut registry: ResMut<SheetRegistry>, // Keep ResMut for popups and cache updates
    ui_feedback: Res<UiFeedbackState>,
) {
    let ctx = contexts.ctx_mut();
    let selected_category_clone = state.selected_category.clone(); // Clone category
    let selected_sheet_name_clone = state.selected_sheet_name.clone(); // Clone sheet name

    // --- Pass state to popups (need mutable access for cache) ---
    // Popups now need ResMut<SheetRegistry> if they need to check existence/metadata during init
    show_column_options_popup( ctx, &mut state, &mut column_rename_writer, &mut column_validator_writer, &mut registry );
    show_rename_popup(ctx, &mut state, &mut rename_event_writer, &ui_feedback);
    show_delete_confirm_popup(ctx, &mut state, &mut delete_event_writer);


    egui::CentralPanel::default().show(ctx, |ui| {
        let text_style = egui::TextStyle::Body;
        let row_height = ui.text_style_height(&text_style) + ui.style().spacing.item_spacing.y;

        // --- Pass state to top panel ---
        show_top_panel(ui, &mut state, &registry, &mut add_row_event_writer, &mut upload_req_writer);

        if !ui_feedback.last_message.is_empty() {
            let text_color = if ui_feedback.is_error { egui::Color32::RED } else { ui.style().visuals.text_color() };
            ui.colored_label(text_color, &ui_feedback.last_message);
        }
        ui.separator();

        // Check if a sheet is selected
        if let Some(selected_name) = &selected_sheet_name_clone {
             // Use the cloned category and sheet name to check registry
             let registry_immut = registry.as_ref(); // Immutable borrow for check/read
             if registry_immut.get_sheet(&selected_category_clone, selected_name).is_none() {
                 ui.vertical_centered(|ui| { ui.label(format!("Sheet '{:?}/{}' no longer exists...", selected_category_clone, selected_name)); });
             } else {
                 // Sheet exists, proceed with rendering
                 let sheet_name_ref = selected_name.as_str();
                 // Get metadata using category and name
                 let render_data_opt = registry_immut
                     .get_sheet(&selected_category_clone, sheet_name_ref)
                     .and_then(|sheet_data| sheet_data.metadata.as_ref().map(|metadata| {
                         (metadata.column_headers.clone(), metadata.column_filters.clone())
                     }));

                 match render_data_opt {
                     Some((headers, filters)) => {
                         let num_cols = headers.len();
                         if num_cols != filters.len() {
                             ui.colored_label(egui::Color32::RED,"Metadata inconsistency...");
                             return;
                         }

                         egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
                             let mut table_builder = TableBuilder::new(ui).striped(true).resizable(true).cell_layout(egui::Layout::left_to_right(egui::Align::Center)).min_scrolled_height(0.0);
                             if num_cols == 0 { table_builder = table_builder.column(Column::remainder().resizable(false)); }
                             else { for _ in 0..num_cols { table_builder = table_builder.column(Column::initial(120.0).at_least(40.0).resizable(true).clip(true)); } }

                             table_builder
                                 .header(20.0, |header_row| {
                                     // Pass mutable state to header (stores category/sheet info for popup)
                                     // sheet_table_header needs category info if popup init needs it
                                     // We'll update the state directly before showing popup
                                     sheet_table_header(header_row, &headers, &filters, sheet_name_ref, &mut state);
                                     // Store category when header clicked
                                     if state.show_column_options_popup && state.column_options_popup_needs_init {
                                         state.options_column_target_category = selected_category_clone.clone();
                                     }

                                 })
                                 .body(|body| {
                                     // Pass immutable registry and mutable state
                                     sheet_table_body(
                                         body,
                                         row_height,
                                         &selected_category_clone, // Pass category
                                         sheet_name_ref,
                                         &headers,
                                         &filters,
                                         registry_immut, // Pass &SheetRegistry
                                         cell_update_writer,
                                         &mut state, // Pass mutable state for cache
                                     );
                                 });
                         }); // End ScrollArea
                     }
                     None => {
                         // Check again if sheet exists but metadata doesn't
                         if registry_immut.get_sheet(&selected_category_clone, sheet_name_ref).is_some() {
                             ui.colored_label(egui::Color32::YELLOW, format!("Metadata missing for sheet '{:?}/{}'.", selected_category_clone, sheet_name_ref));
                         }
                         // Else: the sheet disappeared between check and render (should be rare)
                     }
                 }
             }
        } else {
             // No sheet selected
             if state.selected_category.is_some() {
                 ui.vertical_centered(|ui| { ui.label("Select a sheet from the category."); });
             } else {
                 ui.vertical_centered(|ui| { ui.label("Select a category and sheet, or upload JSON."); });
             }
        }

        // --- Deferred State Clear Logic ---
        let mut needs_clear = false;
        if let Some(name) = &selected_sheet_name_clone {
             // Check using the selected category clone
             if registry.get_sheet(&selected_category_clone, name).is_none() {
                 needs_clear = true; // Selected sheet doesn't exist anymore
             }
        }
        // Also clear if category itself becomes invalid (though less likely unless dirs change)
        // For simplicity, we rely on the sheet check above.

        if needs_clear {
            state.selected_sheet_name = None; // Clear only sheet name first
            // Keep category selected? Or clear both? Let's clear only sheet for now.
            // state.selected_category = None; // Optionally clear category too

            state.show_column_options_popup = false;
            state.show_rename_popup = false;
            state.show_delete_confirm_popup = false;
            // Clear target info in popups
            state.options_column_target_category = None;
            state.options_column_target_sheet.clear();
            state.rename_target_category = None;
            state.rename_target_sheet.clear();
            state.delete_target_category = None;
            state.delete_target_sheet.clear();

            // Clear cache might be too aggressive here, only clear if needed
            // state.linked_column_cache.clear();
            warn!("Selected sheet ('{:?}/{}') removed or invalid, clearing selection state.", selected_category_clone, selected_sheet_name_clone.unwrap_or_default());
        }

    }); // End CentralPanel
}