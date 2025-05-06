// src/ui/elements/top_panel.rs
use crate::sheets::events::{
    AddSheetRowRequest, RequestInitiateFileUpload, RequestDeleteRows,
};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState};
use bevy::prelude::*;
use bevy_egui::egui;

/// Renders the top control panel with sheet selector and action buttons.
pub fn show_top_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry, // Changed to immutable reference
    mut add_row_event_writer: EventWriter<AddSheetRowRequest>,
    mut upload_req_writer: EventWriter<RequestInitiateFileUpload>,
    mut delete_rows_event_writer: EventWriter<RequestDeleteRows>,
) {
    ui.horizontal(|ui| {
        // --- Category Selector ---
        ui.label("Category:");
        let categories = registry.get_categories();
        let selected_category_text = match &state.selected_category {
            Some(cat_name) => cat_name.as_str(),
            None => "Root (Uncategorized)",
        };

        let category_response = egui::ComboBox::from_id_source("category_selector")
            .selected_text(selected_category_text)
            .show_ui(ui, |ui| {
                let is_selected_root = state.selected_category.is_none();
                if ui.selectable_label(is_selected_root, "Root (Uncategorized)").clicked() {
                    if !is_selected_root {
                        state.selected_category = None;
                        state.selected_sheet_name = None;
                        state.ai_selected_rows.clear(); 
                        state.force_filter_recalculation = true; // Invalidate cache
                        debug!("Category changed to Root. Forcing filter recalc.");
                    }
                }
                for cat_opt in categories.iter() {
                    if let Some(cat_name) = cat_opt {
                        let is_selected_cat = state.selected_category.as_deref() == Some(cat_name.as_str());
                        if ui.selectable_label(is_selected_cat, cat_name).clicked() {
                            if !is_selected_cat {
                                state.selected_category = Some(cat_name.clone());
                                state.selected_sheet_name = None;
                                state.ai_selected_rows.clear(); 
                                state.force_filter_recalculation = true; // Invalidate cache
                                debug!("Category changed to '{}'. Forcing filter recalc.", cat_name);
                            }
                        }
                    }
                }
            });
        
        if category_response.response.changed() && state.selected_sheet_name.is_none() {
            // If category changed and no sheet is selected (or became none),
            // ensure force_filter_recalculation is true.
            // This handles cases where the list of sheets might change but selection doesn't auto-pick one.
            state.force_filter_recalculation = true;
        }


        ui.separator();

        // --- Sheet Selector ---
        ui.label("Sheet:");
        let sheets_in_category =
            registry.get_sheet_names_in_category(&state.selected_category);

        ui.add_enabled_ui(!sheets_in_category.is_empty() || state.selected_sheet_name.is_some(), |ui| {
            let selected_sheet_text = state.selected_sheet_name.as_deref().unwrap_or("--Select--");
            let sheet_response = egui::ComboBox::from_id_source("sheet_selector_grid")
                .selected_text(selected_sheet_text)
                .show_ui(ui, |ui| {
                    // Store original selection to detect change
                    let original_selection = state.selected_sheet_name.clone();
                    ui.selectable_value(&mut state.selected_sheet_name, None, "--Select--");
                    for name in sheets_in_category {
                        ui.selectable_value(&mut state.selected_sheet_name, Some(name.clone()), &name);
                    }
                    // If selection changed, invalidate cache
                    if state.selected_sheet_name != original_selection {
                        state.ai_selected_rows.clear(); 
                        state.force_filter_recalculation = true;
                        debug!("Sheet selection changed. Forcing filter recalc.");
                    }
                });

             // Clear selection if sheet becomes invalid (e.g. deleted from another source)
             if let Some(current_sheet_name) = state.selected_sheet_name.as_ref() {
                 if !registry.get_sheet_names_in_category(&state.selected_category).contains(current_sheet_name) {
                     warn!("Selected sheet '{}' no longer valid for category '{:?}'. Clearing selection.", current_sheet_name, state.selected_category);
                     state.selected_sheet_name = None;
                     state.ai_selected_rows.clear(); 
                     state.force_filter_recalculation = true; // Invalidate cache
                 }
             }
        }); 

        let selected_category_cache = state.selected_category.clone(); 
        let selected_sheet_name_cache = state.selected_sheet_name.clone(); 
        let is_sheet_selected = selected_sheet_name_cache.is_some();
        let is_ai_busy = matches!(state.ai_mode, AiModeState::Submitting | AiModeState::Reviewing); 


        if ui.add_enabled(is_sheet_selected, egui::Button::new("✏ Rename")).clicked() {
            if let Some(ref name_to_rename) = selected_sheet_name_cache {
                state.rename_target_category = selected_category_cache.clone();
                state.rename_target_sheet = name_to_rename.clone();
                state.new_name_input = state.rename_target_sheet.clone();
                state.show_rename_popup = true;
            }
        }
        if ui.add_enabled(is_sheet_selected, egui::Button::new("🗑 Delete Sheet")).clicked() {
            if let Some(ref name_to_delete) = selected_sheet_name_cache {
                state.delete_target_category = selected_category_cache.clone();
                state.delete_target_sheet = name_to_delete.clone();
                state.show_delete_confirm_popup = true;
            }
        }
        if ui.add_enabled(is_sheet_selected, egui::Button::new("🧠 AI Rule")).clicked() {
             if let Some(ref sheet_name) = selected_sheet_name_cache {
                 let current_rule = registry
                     .get_sheet(&selected_category_cache, sheet_name)
                     .and_then(|d| d.metadata.as_ref())
                     .and_then(|m| m.ai_general_rule.clone())
                     .unwrap_or_default();
                 state.ai_general_rule_input = current_rule;
                 state.show_ai_rule_popup = true;
             }
        }
        if ui.button("⬆ Upload JSON").on_hover_text("Upload a JSON file (will be placed in Root category)").clicked() {
            upload_req_writer.send(RequestInitiateFileUpload);
        }


        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
             if ui.add_enabled(is_sheet_selected && !is_ai_busy, egui::Button::new("➕ Add Row")).clicked()
             {
                 if let Some(sheet_name) = &state.selected_sheet_name { 
                     add_row_event_writer.send(AddSheetRowRequest {
                         category: state.selected_category.clone(), 
                         sheet_name: sheet_name.clone(),
                     });
                     // Optimistically set flag, system will send SheetDataModifiedEvent too
                    // Set the request flag for scrolling
                    state.request_scroll_to_bottom_on_add = true;
                     state.force_filter_recalculation = true;
                 }
             }

             if ui.button("⚙ Settings").on_hover_text("Configure API Key").clicked() {
                 state.show_settings_popup = true;
             }

            let can_delete_rows = is_sheet_selected && !state.ai_selected_rows.is_empty() && !is_ai_busy;
            let delete_hover_text = if is_sheet_selected && !is_ai_busy {
                "Delete the selected row(s).\n(Enable selection using 'Prepare for AI')"
            } else if !is_sheet_selected {
                 "Select a sheet first"
            } else if is_ai_busy {
                 "Cannot delete rows during AI processing"
            } else { 
                 "Select rows first using 'Prepare for AI'"
            };
            if ui.add_enabled(can_delete_rows, egui::Button::new("🗑 Delete Rows"))
                .on_hover_text(delete_hover_text) 
                .clicked()
            {
                if let Some(sheet_name) = &selected_sheet_name_cache {
                    delete_rows_event_writer.send(RequestDeleteRows {
                         category: selected_category_cache.clone(),
                         sheet_name: sheet_name.clone(),
                         row_indices: state.ai_selected_rows.clone(), 
                    });
                    state.ai_selected_rows.clear();
                    if state.ai_mode == AiModeState::Preparing {
                        state.ai_mode = AiModeState::Idle;
                    }
                    // Optimistically set flag
                    state.force_filter_recalculation = true;
                }
            }
             
              if state.ai_mode == AiModeState::Idle {
                   if ui.add_enabled(is_sheet_selected && !is_ai_busy, egui::Button::new("✨ Prepare for AI"))
                     .on_hover_text("Enable row selection checkboxes for AI or Deletion") 
                     .clicked() {
                        state.ai_mode = AiModeState::Preparing;
                        state.ai_selected_rows.clear(); 
                   }
              }
        }); 
    }); 
}