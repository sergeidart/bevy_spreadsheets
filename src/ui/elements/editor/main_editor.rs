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
    mut registry: ResMut<SheetRegistry>, // Keep ResMut for popups
    ui_feedback: Res<UiFeedbackState>,
) {
    let ctx = contexts.ctx_mut();
    let selected_sheet_name_clone = state.selected_sheet_name.clone();

    // --- Pass state to popups (need mutable access for cache) ---
    // Note: Popups already took mutable state, but now it's needed for cache too.
    show_column_options_popup( ctx, &mut state, &mut column_rename_writer, &mut column_validator_writer, &mut registry );
    show_rename_popup(ctx, &mut state, &mut rename_event_writer, &ui_feedback);
    show_delete_confirm_popup(ctx, &mut state, &mut delete_event_writer);


    egui::CentralPanel::default().show(ctx, |ui| {
        let text_style = egui::TextStyle::Body;
        let row_height = ui.text_style_height(&text_style) + ui.style().spacing.item_spacing.y;

        // --- Pass state to top panel (no cache needed here, immutable registry is fine) ---
        show_top_panel(ui, &mut state, &registry, &mut add_row_event_writer, &mut upload_req_writer);

        if !ui_feedback.last_message.is_empty() {
            let text_color = if ui_feedback.is_error { egui::Color32::RED } else { ui.style().visuals.text_color() };
            ui.colored_label(text_color, &ui_feedback.last_message);
        }
        ui.separator();

        if let Some(selected_name) = &selected_sheet_name_clone {
             if registry.get_sheet(selected_name).is_none() {
                 ui.vertical_centered(|ui| { ui.label(format!("Sheet '{}' no longer exists...", selected_name)); });
             } else {
                 let sheet_name_ref = selected_name.as_str();
                 let render_data_opt = registry
                     .get_sheet(sheet_name_ref)
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
                                     // Pass mutable state to header in case it needs cache access later
                                     sheet_table_header(header_row, &headers, &filters, sheet_name_ref, &mut state);
                                 })
                                 .body(|body| {
                                     // --- MODIFIED: Pass mutable state to sheet_table_body ---
                                     sheet_table_body(
                                         body,
                                         row_height,
                                         sheet_name_ref,
                                         &headers,
                                         &filters,
                                         registry.as_ref(), // Pass &SheetRegistry
                                         cell_update_writer,
                                         &mut state, // Pass mutable state here
                                     );
                                     // --- End Modification ---
                                 });
                         }); // End ScrollArea
                     }
                     None => { if registry.get_sheet(sheet_name_ref).is_some() { ui.colored_label(egui::Color32::YELLOW, format!("Metadata missing for sheet '{}'.", sheet_name_ref)); } }
                 }
             }
        } else {
             ui.vertical_centered(|ui| { ui.label("Select a sheet or upload JSON."); });
        }

        // Deferred State Clear Logic
        let mut needs_clear = false;
        if let Some(name) = &selected_sheet_name_clone {
             if registry.get_sheet(name).is_none() { needs_clear = true; }
        }
        if needs_clear {
            state.selected_sheet_name = None;
            state.show_column_options_popup = false;
            state.show_rename_popup = false;
            state.show_delete_confirm_popup = false;
            state.options_column_target_sheet.clear();
            state.rename_target.clear();
            state.delete_target.clear();
            // --- Clear the cache when selection is cleared ---
            // (Could also clear selectively based on deleted/renamed sheets)
            state.linked_column_cache.clear();
            warn!("Selected sheet removed, clearing state and linked column cache.");
        }

    }); // End CentralPanel

    // Popups shown after CentralPanel
}