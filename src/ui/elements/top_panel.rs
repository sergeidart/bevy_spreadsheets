// src/ui/elements/top_panel.rs
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::HashSet; // Added HashSet for potentially collecting unique names later if needed

use crate::sheets::resources::SheetRegistry;
use crate::sheets::events::{
    AddSheetRowRequest,
    RequestRenameSheet, RequestDeleteSheet,
    RequestInitiateFileUpload,
};
use super::editor::EditorWindowState;

/// Renders the top control panel with sheet selector and action buttons.
pub fn show_top_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    add_row_event_writer: &mut EventWriter<AddSheetRowRequest>,
    upload_req_writer: &mut EventWriter<RequestInitiateFileUpload>,
) {
    ui.horizontal(|ui| {
        // --- Category Selector ---
        ui.label("Category:");
        let categories = registry.get_categories();
        let selected_category_text = match &state.selected_category {
            Some(cat_name) => cat_name.as_str(),
            None => "Root (Uncategorized)", // Display text for None category
        };

        let category_response = egui::ComboBox::from_id_source("category_selector")
            .selected_text(selected_category_text)
            .show_ui(ui, |ui| {
                // Explicitly add the 'None' (Root) option first
                let is_selected = state.selected_category.is_none();
                 if ui.selectable_label(is_selected, "Root (Uncategorized)").clicked() {
                    if !is_selected { // Only change if not already selected
                        state.selected_category = None;
                        state.selected_sheet_name = None; // Clear sheet selection when category changes
                    }
                 }
                // Add other categories
                for cat_opt in categories.iter() {
                     if let Some(cat_name) = cat_opt { // Skip the None we handled above
                         let is_selected = state.selected_category.as_deref() == Some(cat_name.as_str());
                         if ui.selectable_label(is_selected, cat_name).clicked() {
                             if !is_selected {
                                 state.selected_category = Some(cat_name.clone());
                                 state.selected_sheet_name = None; // Clear sheet selection
                             }
                         }
                     }
                 }
            });

        ui.separator();

        // --- Sheet Selector (depends on category) ---
        ui.label("Sheet:");
        let sheets_in_category = registry.get_sheet_names_in_category(&state.selected_category);

        // <<< --- FIX: Delay immutable borrow for selected_text --- >>>
        ui.add_enabled_ui(!sheets_in_category.is_empty() || state.selected_sheet_name.is_some(), |ui| {
             // Determine selected text *inside* this closure, just before creating ComboBox
             let selected_sheet_text = state.selected_sheet_name.as_deref().unwrap_or("--Select--");

             let sheet_response = egui::ComboBox::from_id_source("sheet_selector_grid")
                .selected_text(selected_sheet_text) // Use the text determined above
                .show_ui(ui, |ui| {
                    // Use mutable borrow here - this is now fine
                    ui.selectable_value(&mut state.selected_sheet_name, None, "--Select--");
                    for name in sheets_in_category { // sheets_in_category captured immutably is fine
                        ui.selectable_value(&mut state.selected_sheet_name, Some(name.clone()), &name);
                    }
                });

             // Handle clearing selection if category changes made the current sheet invalid
             // Check AFTER the ComboBox interaction
             if !sheet_response.response.changed() && state.selected_sheet_name.is_some() {
                let current_sheet_name = state.selected_sheet_name.as_ref().unwrap();
                // Need to get sheets in category *again* in case category just changed
                if !registry.get_sheet_names_in_category(&state.selected_category).contains(current_sheet_name) {
                     warn!("Selected sheet '{}' no longer valid for category '{:?}'. Clearing selection.", current_sheet_name, state.selected_category);
                     state.selected_sheet_name = None;
                }
             }
        }); // End add_enabled_ui for Sheet Selector


        // --- Action Buttons ---
        let selected_category_cache = state.selected_category.clone();
        let selected_sheet_name_cache = state.selected_sheet_name.clone();
        let is_sheet_selected = selected_sheet_name_cache.is_some();

        if ui
            .add_enabled(is_sheet_selected, egui::Button::new("✏ Rename"))
            .clicked()
        {
            if let Some(ref name_to_rename) = selected_sheet_name_cache {
                state.rename_target_category = selected_category_cache.clone(); // Store category
                state.rename_target_sheet = name_to_rename.clone();
                state.new_name_input = state.rename_target_sheet.clone();
                state.show_rename_popup = true;
            }
        }
        if ui
            .add_enabled(is_sheet_selected, egui::Button::new("🗑 Delete"))
            .clicked()
        {
            if let Some(ref name_to_delete) = selected_sheet_name_cache {
                state.delete_target_category = selected_category_cache.clone(); // Store category
                state.delete_target_sheet = name_to_delete.clone();
                state.show_delete_confirm_popup = true;
            }
        }

        // Upload button remains global
        if ui
            .button("⬆ Upload JSON")
            .on_hover_text("Upload a JSON file (will be placed in Root category)") // Clarify behavior
            .clicked()
        {
            upload_req_writer.send(RequestInitiateFileUpload);
        }

        // Spacer + Right-aligned buttons
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Add Row Button
            if ui
                .add_enabled(is_sheet_selected, egui::Button::new("➕ Add Row"))
                .clicked()
            {
                if let Some(sheet_name) = &state.selected_sheet_name {
                    add_row_event_writer.send(AddSheetRowRequest {
                        category: selected_category_cache.clone(), // Send category
                        sheet_name: sheet_name.clone(),
                    });
                }
            }
        });
    }); // End Top Controls Horizontal layout
}