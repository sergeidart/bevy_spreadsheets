// src/ui/elements/top_panel.rs
use crate::sheets::events::{
    AddSheetRowRequest, RequestDeleteRows, RequestInitiateFileUpload,
};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState};
use bevy::prelude::*;
use bevy_egui::egui;
// Removed: use std::path::Path; // Not directly used

use crate::visual_copier::{
    resources::VisualCopierManager,
    events::{
        PickFolderRequest, QueueTopPanelCopyEvent, ReverseTopPanelFoldersEvent,
    },
};

/// Helper function to truncate a path string to fit within a given pixel width.
fn truncate_path_string(path_str: &str, max_width_pixels: f32, ui: &egui::Ui) -> String {
    if path_str.is_empty() {
        return "".to_string();
    }
    let font_id_val = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui.fonts(|f| f.layout_no_wrap(path_str.to_string(), font_id_val.clone(), egui::Color32::PLACEHOLDER));

    if galley.size().x <= max_width_pixels {
        return path_str.to_string();
    }

    let ellipsis = "...";
    let ellipsis_width = ui.fonts(|f| f.layout_no_wrap(ellipsis.to_string(), font_id_val.clone(), egui::Color32::PLACEHOLDER)).size().x;

    if ellipsis_width > max_width_pixels {
        let mut fitting_ellipsis = String::new();
        let mut current_ellipsis_width = 0.0;
        for c in ellipsis.chars() {
            let char_s = c.to_string();
            let char_w = ui.fonts(|f| f.layout_no_wrap(char_s.clone(), font_id_val.clone(), egui::Color32::PLACEHOLDER)).size().x;
            if current_ellipsis_width + char_w <= max_width_pixels {
                fitting_ellipsis.push(c);
                current_ellipsis_width += char_w;
            } else {
                break;
            }
        }
        return fitting_ellipsis;
    }

    let mut truncated_len = 0;
    let mut current_width = 0.0;

    for (idx, char_instance) in path_str.char_indices() {
        let char_s = match path_str.get(idx..idx + char_instance.len_utf8()) {
            Some(s) => s,
            None => break,
        };
        let char_w = ui.fonts(|f| f.layout_no_wrap(char_s.to_string(), font_id_val.clone(), egui::Color32::PLACEHOLDER)).size().x;

        if current_width + char_w + ellipsis_width > max_width_pixels {
            break;
        }
        current_width += char_w;
        truncated_len = idx + char_instance.len_utf8();
    }

    if truncated_len == 0 && !path_str.is_empty() {
        return ellipsis.to_string();
    } else if path_str.is_empty() {
        return "".to_string();
    }

    format!("{}{}", &path_str[..truncated_len], ellipsis)
}


#[allow(clippy::too_many_arguments)]
pub fn show_top_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    registry: &SheetRegistry,
    mut add_row_event_writer: EventWriter<AddSheetRowRequest>,
    mut upload_req_writer: EventWriter<RequestInitiateFileUpload>,
    mut delete_rows_event_writer: EventWriter<RequestDeleteRows>,
    mut copier_manager: ResMut<VisualCopierManager>,
    mut pick_folder_writer: EventWriter<PickFolderRequest>,
    mut queue_top_panel_copy_writer: EventWriter<QueueTopPanelCopyEvent>,
    mut reverse_folders_writer: EventWriter<ReverseTopPanelFoldersEvent>,
) {
    egui::TopBottomPanel::top("main_top_controls_panel")
        .show_inside(ui, |ui| {
            // --- Row 1: Sheet Management and Quick Copy ---
            ui.horizontal(|ui| {
                // Category Selector
                ui.label("Category:");
                let categories = registry.get_categories();
                let selected_category_text = state.selected_category.as_deref().unwrap_or("Root (Uncategorized)");

                let category_response = egui::ComboBox::from_id_source("category_selector_top_panel")
                    .selected_text(selected_category_text)
                    .show_ui(ui, |ui| {
                        let is_selected_root = state.selected_category.is_none();
                        if ui.selectable_label(is_selected_root, "Root (Uncategorized)").clicked() {
                            if !is_selected_root {
                                state.selected_category = None;
                                state.selected_sheet_name = None;
                                state.reset_selection_modes();
                                state.force_filter_recalculation = true;
                                state.ai_rule_popup_needs_init = true;
                            }
                        }
                        for cat_opt in categories.iter() {
                            if let Some(cat_name) = cat_opt {
                                let is_selected_cat = state.selected_category.as_deref() == Some(cat_name.as_str());
                                if ui.selectable_label(is_selected_cat, cat_name).clicked() {
                                    if !is_selected_cat {
                                        state.selected_category = Some(cat_name.clone());
                                        state.selected_sheet_name = None;
                                        state.reset_selection_modes();
                                        state.force_filter_recalculation = true;
                                        state.ai_rule_popup_needs_init = true;
                                    }
                                }
                            }
                        }
                    });

                if category_response.response.changed() {
                    state.force_filter_recalculation = true;
                    state.ai_rule_popup_needs_init = true;
                }

                // Upload Json Button (Second item)
                if ui.button("⬆ Upload JSON").on_hover_text("Upload a JSON file (will be placed in Root category)").clicked() {
                    upload_req_writer.send(RequestInitiateFileUpload);
                }

                ui.separator();

                // Sheet Selector
                ui.label("Sheet:");
                let sheets_in_category = registry.get_sheet_names_in_category(&state.selected_category);
                ui.add_enabled_ui(!sheets_in_category.is_empty() || state.selected_sheet_name.is_some(), |ui| {
                    let selected_sheet_text = state.selected_sheet_name.as_deref().unwrap_or("--Select--");
                    let sheet_response = egui::ComboBox::from_id_source("sheet_selector_top_panel")
                        .selected_text(selected_sheet_text)
                        .show_ui(ui, |ui| {
                            let original_selection = state.selected_sheet_name.clone();
                            ui.selectable_value(&mut state.selected_sheet_name, None, "--Select--");
                            for name in sheets_in_category {
                                ui.selectable_value(&mut state.selected_sheet_name, Some(name.clone()), &name);
                            }
                            if state.selected_sheet_name != original_selection {
                                state.reset_selection_modes();
                                state.force_filter_recalculation = true;
                                state.ai_rule_popup_needs_init = true;
                            }
                        });
                    if sheet_response.response.changed() {
                         state.force_filter_recalculation = true;
                         state.ai_rule_popup_needs_init = true;
                    }

                    if let Some(current_sheet_name) = state.selected_sheet_name.as_ref() {
                        if !registry.get_sheet_names_in_category(&state.selected_category).contains(current_sheet_name) {
                            state.selected_sheet_name = None;
                            state.reset_selection_modes();
                            state.force_filter_recalculation = true;
                            state.ai_rule_popup_needs_init = true;
                        }
                    }
                });

                let is_sheet_selected = state.selected_sheet_name.is_some();
                let can_interact_with_sheet = is_sheet_selected && state.ai_mode == AiModeState::Idle && !state.delete_row_mode_active;

                // Rename Button
                if ui.add_enabled(can_interact_with_sheet, egui::Button::new("✏ Rename")).clicked() {
                    if let Some(ref name_to_rename) = state.selected_sheet_name {
                        state.rename_target_category = state.selected_category.clone();
                        state.rename_target_sheet = name_to_rename.clone();
                        state.new_name_input = state.rename_target_sheet.clone();
                        state.show_rename_popup = true;
                    }
                }

                // Delete Sheet Button
                if ui.add_enabled(can_interact_with_sheet, egui::Button::new("🗑 Delete Sheet")).clicked() {
                    if let Some(ref name_to_delete) = state.selected_sheet_name {
                        state.delete_target_category = state.selected_category.clone();
                        state.delete_target_sheet = name_to_delete.clone();
                        state.show_delete_confirm_popup = true;
                    }
                }

                // Copy Button
                if ui.button("📋 Copy").clicked() {
                    state.show_quick_copy_bar = !state.show_quick_copy_bar;
                }
            });

            // Quick Copy Bar (conditionally rendered)
            if state.show_quick_copy_bar {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        const MAX_PATH_DISPLAY_WIDTH: f32 = 250.0;
                        ui.label("Quick Copy:");

                        if ui.button("FROM").on_hover_text("Select source folder").clicked() {
                            pick_folder_writer.send(PickFolderRequest { for_task_id: None, is_start_folder: true });
                        }
                        let from_path_str = copier_manager.top_panel_from_folder.as_ref().map_or_else(
                            || "None".to_string(),
                            |p| p.display().to_string()
                        );
                        ui.label(truncate_path_string(&from_path_str, MAX_PATH_DISPLAY_WIDTH, ui))
                          .on_hover_text(&from_path_str);

                        if ui.button("TO").on_hover_text("Select destination folder").clicked() {
                            pick_folder_writer.send(PickFolderRequest { for_task_id: None, is_start_folder: false });
                        }
                        let to_path_str = copier_manager.top_panel_to_folder.as_ref().map_or_else(
                            || "None".to_string(),
                            |p| p.display().to_string()
                        );
                        ui.label(truncate_path_string(&to_path_str, MAX_PATH_DISPLAY_WIDTH, ui))
                          .on_hover_text(&to_path_str);

                        if ui.button("Swap ↔").clicked() {
                            reverse_folders_writer.send(ReverseTopPanelFoldersEvent);
                        }

                        let can_quick_copy = copier_manager.top_panel_from_folder.is_some() && copier_manager.top_panel_to_folder.is_some();
                        if ui.add_enabled(can_quick_copy, egui::Button::new("COPY")).clicked() {
                            queue_top_panel_copy_writer.send(QueueTopPanelCopyEvent);
                        }
                        ui.label(&copier_manager.top_panel_copy_status);
                    });
                });
            }
            ui.separator();

            // --- Row 2: AI Operations and Row Management ---
            ui.horizontal(|ui| {
                let is_sheet_selected = state.selected_sheet_name.is_some();

                // --- AI Section ---
                match state.ai_mode {
                    AiModeState::Idle => {
                        if !state.delete_row_mode_active { // Only show if not in delete mode
                            if ui.add_enabled(is_sheet_selected, egui::Button::new("✨ Prepare for AI")).clicked() {
                                state.ai_mode = AiModeState::Preparing;
                                state.ai_selected_rows.clear();
                            }
                        } else {
                            // Optionally show a disabled button or nothing when delete mode is active
                             ui.add_enabled(false, egui::Button::new("✨ Prepare for AI"))
                                .on_disabled_hover_text("Cancel Delete Row mode first");
                        }
                    }
                    AiModeState::Preparing => {
                        ui.label(egui::RichText::new("AI Mode: Preparing Selection").color(egui::Color32::LIGHT_BLUE));
                        // Edit AI Config Button
                        if ui.button("Edit AI Config").on_hover_text("Edit sheet-specific AI model, rules, and parameters").clicked() {
                            state.show_ai_rule_popup = true;
                            state.ai_rule_popup_needs_init = true;
                        }
                        // General Settings Button (API Key)
                        if ui.button("⚙ Settings").on_hover_text("Configure API Key (Session)").clicked() {
                            state.show_settings_popup = true;
                        }
                        // Cancel AI Mode Button
                        if ui.button("Cancel AI Mode").clicked() {
                            state.reset_selection_modes();
                        }
                        // "Send to AI" button is shown in the ai_control_panel below
                    }
                    AiModeState::Submitting => {
                        ui.label(egui::RichText::new("AI Mode: Submitting...").color(egui::Color32::LIGHT_BLUE));
                        // Maybe add a cancel button here later if task cancellation is implemented
                    }
                    AiModeState::ResultsReady => {
                        ui.label(egui::RichText::new("AI Mode: Results Ready").color(egui::Color32::LIGHT_GREEN));
                        // "Review Suggestions" button shown in ai_control_panel
                         if ui.button("Cancel AI Mode").clicked() { // Allow cancelling before review
                            state.reset_selection_modes();
                        }
                    }
                     AiModeState::Reviewing => {
                        ui.label(egui::RichText::new("AI Mode: Reviewing...").color(egui::Color32::YELLOW));
                         // Cancel/Apply/Skip shown in review panel
                    }
                }
                // --- End AI Section ---

                ui.separator();

                // --- Delete Row Section ---
                if state.ai_mode == AiModeState::Idle { // Only allow entering delete mode if AI is Idle
                    if ui.add_enabled(is_sheet_selected, egui::Button::new("🗑️ Delete Row")).clicked() {
                        state.delete_row_mode_active = !state.delete_row_mode_active;
                        if state.delete_row_mode_active {
                            state.ai_selected_rows.clear();
                        } else {
                            state.ai_selected_rows.clear();
                        }
                    }
                    if state.delete_row_mode_active {
                        ui.label(egui::RichText::new("Row Deletion Mode").color(egui::Color32::YELLOW));
                        let can_delete_selected_rows = is_sheet_selected && !state.ai_selected_rows.is_empty();
                        if ui.add_enabled(can_delete_selected_rows, egui::Button::new("Delete Selected")).clicked() {
                             if let Some(sheet_name) = &state.selected_sheet_name {
                                delete_rows_event_writer.send(RequestDeleteRows {
                                    category: state.selected_category.clone(),
                                    sheet_name: sheet_name.clone(),
                                    row_indices: state.ai_selected_rows.clone(),
                                });
                                state.ai_selected_rows.clear();
                                state.delete_row_mode_active = false;
                                state.force_filter_recalculation = true;
                            }
                        }
                        if ui.button("Cancel Delete").clicked() {
                            state.delete_row_mode_active = false;
                            state.ai_selected_rows.clear();
                        }
                    }
                } else {
                     // Show disabled Delete Row button if AI mode is active
                     ui.add_enabled(false, egui::Button::new("🗑️ Delete Row"))
                        .on_disabled_hover_text("Cancel AI mode first");
                }
                // --- End Delete Row Section ---

                ui.separator();

                // --- Add Row Section ---
                let can_add_row = is_sheet_selected && state.ai_mode == AiModeState::Idle && !state.delete_row_mode_active;
                if ui.add_enabled(can_add_row, egui::Button::new("➕ Add Row")).clicked() {
                    if let Some(sheet_name) = &state.selected_sheet_name {
                        add_row_event_writer.send(AddSheetRowRequest {
                            category: state.selected_category.clone(),
                            sheet_name: sheet_name.clone(),
                        });
                        state.request_scroll_to_bottom_on_add = true;
                        state.force_filter_recalculation = true;
                    }
                }
                // --- End Add Row Section ---

                // Removed the general settings button from the right end as it's now part of the AI Mode Preparing state
                // ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                //     // ... settings button was here ...
                // });
            });
        });
}