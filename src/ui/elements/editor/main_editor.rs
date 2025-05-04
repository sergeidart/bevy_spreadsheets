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
use crate::sheets::systems::io::save::save_single_sheet;
use crate::sheets::{
    resources::SheetRegistry,
    events::{
        AddSheetRowRequest, RequestRenameSheet, RequestDeleteSheet,
        RequestInitiateFileUpload, RequestUpdateColumnName,
    },
};
use crate::ui::UiFeedbackState;


/// The main UI assembly function for the sheet editor.
/// Orchestrates the top panel, feedback, table, and popups.
pub fn generic_sheet_editor_ui(
    mut contexts: EguiContexts,
    mut state: Local<EditorWindowState>,
    // Event writers
    mut add_row_event_writer: EventWriter<AddSheetRowRequest>,
    mut rename_event_writer: EventWriter<RequestRenameSheet>,
    mut delete_event_writer: EventWriter<RequestDeleteSheet>,
    mut upload_req_writer: EventWriter<RequestInitiateFileUpload>,
    mut column_rename_writer: EventWriter<RequestUpdateColumnName>,
    // Resources
    mut registry: ResMut<SheetRegistry>,
    ui_feedback: Res<UiFeedbackState>,
) {
    let ctx = contexts.ctx_mut();

    // --- Handle Save Flag (Moved to end, after UI logic) ---

    // --- Pre-read immutable state needed later ---
    // Clone the selected sheet name *before* the main UI closure to avoid borrow conflict
    let selected_sheet_name_clone = state.selected_sheet_name.clone();

    // --- Main Panel UI ---
    egui::CentralPanel::default().show(ctx, |ui| {
        // --- Calculate row height using the main ui context ---
        let text_style = egui::TextStyle::Body;
        let row_height = ui.text_style_height(&text_style) + ui.style().spacing.item_spacing.y;

        // --- Top Panel ---
        show_top_panel(
            ui,
            &mut state, // Pass mutable state here (allowed before it's borrowed immutably later)
            &registry,
            &mut add_row_event_writer,
            &mut upload_req_writer,
        );

        // --- Feedback Display ---
        if !ui_feedback.last_message.is_empty() {
            let text_color = if ui_feedback.is_error {
                egui::Color32::RED
            } else {
                ui.style().visuals.text_color()
            };
            ui.colored_label(text_color, &ui_feedback.last_message);
        }
        ui.separator();

        // --- Sheet Selection Logic ---
        // Use the cloned name here
        if let Some(selected_name) = &selected_sheet_name_clone {
            let mut clear_selection = false;
            if registry.get_sheet(selected_name).is_none() {
                info!(
                    "Selected sheet '{}' no longer exists in registry, clearing selection.",
                    selected_name
                );
                clear_selection = true;
            }
            // Check if clear_selection flag was set and update state accordingly
            // This needs mutable access to state, potential conflict?
            // Let's defer the state update *outside* the table rendering block
            // For now, just use the selected_name directly if it exists.
            // This section needs careful review regarding state mutation timing.

             if clear_selection {
                // This mutable borrow conflicts if clear_selection is set.
                // Let's handle clearing *after* the table view or rethink.
                // **Deferring the state clear:**
                // We will check clear_selection after the 'if let Some(selected_name)' block.
             } else {
                 // --- Table View Area (only if sheet exists and is selected) ---
                 let sheet_name_ref = selected_name.as_str();

                 // Get data needed for rendering (headers, types, filters)
                 let render_data_opt = registry
                     .get_sheet(sheet_name_ref)
                     .and_then(|sheet_data| {
                         sheet_data.metadata.as_ref().map(|metadata| {
                             (
                                 metadata.column_headers.clone(), // Clone necessary data
                                 metadata.column_types.clone(),
                                 metadata.column_filters.clone(),
                             )
                         })
                     });

                 match render_data_opt {
                     Some((headers, column_types, filters)) => {
                         let num_cols = headers.len();
                         if num_cols != column_types.len() || num_cols != filters.len() {
                             ui.colored_label(
                                 egui::Color32::RED,
                                 "Internal Error: Metadata inconsistency (headers/types/filters length mismatch).",
                             );
                             return; // Stop rendering this part if inconsistent
                         }

                         // --- Table Rendering ---
                         egui::ScrollArea::both()
                             .auto_shrink([false; 2])
                             .show(ui, |ui| {
                                 let mut table_builder = TableBuilder::new(ui)
                                     .striped(true)
                                     .resizable(true)
                                     .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                     .min_scrolled_height(0.0);

                                 // Setup columns
                                 if num_cols == 0 {
                                     table_builder =
                                         table_builder.column(Column::remainder().resizable(false));
                                 } else {
                                     for _ in 0..num_cols {
                                         table_builder = table_builder.column(
                                             Column::initial(120.0)
                                                 .at_least(40.0)
                                                 .resizable(true)
                                                 .clip(true),
                                         );
                                     }
                                 }

                                 // Build the table using the dedicated header and body functions
                                 table_builder
                                     .header(20.0, |header_row| {
                                         // Pass mutable state here - THIS IS THE PROBLEM POINT
                                         // The outer closure already borrowed state immutably via selected_sheet_name_clone
                                         // We need to avoid borrowing `state` mutably here if it's still borrowed immutably.
                                         // WORKAROUND: Let header click only store target index/name in state, handle popup opening *outside* table closure?
                                         // For now, let's try passing state: &mut EditorWindowState again and see if cloning name fixed it.
                                         sheet_table_header(
                                             header_row,
                                             &headers,
                                             &filters,
                                             sheet_name_ref,
                                             &mut state, // THIS IS THE PROBLEMATIC MUTABLE BORROW
                                         );
                                     })
                                     .body(|body| {
                                         sheet_table_body(
                                             body,
                                             row_height, // Pass calculated height
                                             sheet_name_ref,
                                             &headers,
                                             &column_types,
                                             &filters,
                                             &mut registry, // Pass mutable registry for edits
                                             &mut state, // Pass mutable state for save flagging
                                         );
                                     });
                             }); // End ScrollArea
                     }
                     None => {
                         // Metadata missing or sheet disappeared handling
                         if registry.get_sheet(sheet_name_ref).is_some() {
                             ui.colored_label(
                                 egui::Color32::YELLOW,
                                 format!("Metadata missing for sheet '{}'.", sheet_name_ref),
                             );
                         }
                         // If sheet is gone, the outer 'if let Some' won't match anyway after clear logic.
                     }
                 } // end match render_data_opt
             } // end if !clear_selection
        } else {
             // No sheet selected
             ui.vertical_centered(|ui| {
                 ui.label("Select a sheet or upload JSON.");
             });
        } // end if let Some(selected_name)

        // --- Deferred State Update for Clear Selection ---
        // Check the flag *after* the main table logic that might have borrowed state
        let mut needs_clear = false;
        if let Some(name) = &selected_sheet_name_clone {
             if registry.get_sheet(name).is_none() {
                needs_clear = true;
             }
        }
        if needs_clear {
            // Now it should be safe to borrow state mutably
            state.selected_sheet_name = None;
            state.show_column_options_popup = false;
            state.options_column_target_sheet.clear();
            state.options_column_target_index = 0;
            state.options_column_rename_input.clear();
            state.options_column_filter_input.clear();
            state.sheet_needs_save = false;
            state.sheet_to_save.clear();
        }


        // End CentralPanel content
    }); // End CentralPanel::show

    // --- Show Popups ---
    // These run *after* the CentralPanel closure, so borrow checking relative to that is okay.
    show_column_options_popup(
        ctx,
        &mut state,
        &mut column_rename_writer,
        &mut registry,
    );
    show_rename_popup(ctx, &mut state, &mut rename_event_writer, &ui_feedback);
    show_delete_confirm_popup(ctx, &mut state, &mut delete_event_writer);

    // --- Handle Flagged Save (at the end) ---
    if state.sheet_needs_save && !state.sheet_to_save.is_empty() {
        info!(
            "Triggering save for sheet '{}' due to flag.",
            state.sheet_to_save
        );
        // Perform save using immutable borrow
        let registry_immut = registry.as_ref(); // Reborrow immutably
        save_single_sheet(registry_immut, &state.sheet_to_save);
        // Reset flag *after* successful save attempt
        state.sheet_needs_save = false;
        state.sheet_to_save.clear();
    }
}