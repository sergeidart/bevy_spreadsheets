// src/ui/elements/top_panel.rs
use crate::sheets::events::{
    AddSheetRowRequest, RequestDeleteRows, RequestInitiateFileUpload,
};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState};
// --- MODIFIED: Remove AppExit import ---
// use bevy::{app::AppExit, prelude::*};
use bevy::prelude::*;
// --- END MODIFIED ---
use bevy_egui::egui;

use crate::visual_copier::{
    resources::VisualCopierManager,
    events::{
        PickFolderRequest, QueueTopPanelCopyEvent, ReverseTopPanelFoldersEvent,
        VisualCopierStateChanged,
        // --- ADDED: Import RequestAppExit ---
        RequestAppExit,
        // --- END ADDED ---
    },
};

// --- truncate_path_string function remains the same ---
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
    mut copier_manager: ResMut<VisualCopierManager>,
    mut pick_folder_writer: EventWriter<PickFolderRequest>,
    mut queue_top_panel_copy_writer: EventWriter<QueueTopPanelCopyEvent>,
    mut reverse_folders_writer: EventWriter<ReverseTopPanelFoldersEvent>,
    // --- MODIFIED: Changed parameter type ---
    // mut app_exit_writer: EventWriter<AppExit>,
    mut request_app_exit_writer: EventWriter<RequestAppExit>,
    // --- END MODIFIED ---
    mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    egui::TopBottomPanel::top("main_top_controls_panel")
        .show_inside(ui, |ui| {
            // --- Row 1: Sheet Management and Quick Copy ---
            ui.horizontal(|ui| {
                // (Sheet selection logic remains the same)
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

                if ui.button("⬆ Upload JSON").on_hover_text("Upload a JSON file (will be placed in Root category)").clicked() {
                    upload_req_writer.send(RequestInitiateFileUpload);
                }
                ui.separator();
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

                if ui.add_enabled(can_interact_with_sheet, egui::Button::new("✏ Rename")).clicked() {
                    if let Some(ref name_to_rename) = state.selected_sheet_name {
                        state.rename_target_category = state.selected_category.clone();
                        state.rename_target_sheet = name_to_rename.clone();
                        state.new_name_input = state.rename_target_sheet.clone();
                        state.show_rename_popup = true;
                    }
                }
                if ui.add_enabled(can_interact_with_sheet, egui::Button::new("🗑 Delete Sheet")).clicked() {
                    if let Some(ref name_to_delete) = state.selected_sheet_name {
                        state.delete_target_category = state.selected_category.clone();
                        state.delete_target_sheet = name_to_delete.clone();
                        state.show_delete_confirm_popup = true;
                    }
                }

                // Copy Button (Toggle)
                let copy_button_text = if state.show_quick_copy_bar { "❌ Close Copy" } else { "📋 Copy" };
                if ui.button(copy_button_text).clicked() {
                    state.show_quick_copy_bar = !state.show_quick_copy_bar;
                }

                // App Exit Button
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // --- MODIFIED: Send RequestAppExit ---
                    if ui.add(egui::Button::new("❌ App Exit")).clicked() {
                        info!("'App Exit' button clicked. Sending RequestAppExit event.");
                        request_app_exit_writer.send(RequestAppExit); // Send RequestAppExit
                    }
                    // --- END MODIFIED ---
                });
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
                        let from_path_str = copier_manager.top_panel_from_folder.as_ref().map_or_else(|| "None".to_string(), |p| p.display().to_string());
                        ui.label(truncate_path_string(&from_path_str, MAX_PATH_DISPLAY_WIDTH, ui)).on_hover_text(&from_path_str);

                        if ui.button("TO").on_hover_text("Select destination folder").clicked() {
                            pick_folder_writer.send(PickFolderRequest { for_task_id: None, is_start_folder: false });
                        }
                        let to_path_str = copier_manager.top_panel_to_folder.as_ref().map_or_else(|| "None".to_string(), |p| p.display().to_string());
                        ui.label(truncate_path_string(&to_path_str, MAX_PATH_DISPLAY_WIDTH, ui)).on_hover_text(&to_path_str);

                        if ui.button("Swap ↔").clicked() {
                            reverse_folders_writer.send(ReverseTopPanelFoldersEvent);
                        }

                        let can_quick_copy = copier_manager.top_panel_from_folder.is_some() && copier_manager.top_panel_to_folder.is_some();
                        if ui.add_enabled(can_quick_copy, egui::Button::new("COPY")).clicked() {
                            queue_top_panel_copy_writer.send(QueueTopPanelCopyEvent);
                        }
                        ui.label(&copier_manager.top_panel_copy_status);

                        ui.separator();

                        // --- Checkbox interaction sends event ---
                        let checkbox_response = ui.checkbox(&mut copier_manager.copy_top_panel_on_exit, "Copy on Exit")
                            .on_hover_text("If checked, performs this Quick Copy operation synchronously just before the application closes via 'App Exit'.");

                        if checkbox_response.changed() {
                            state_changed_writer.send(VisualCopierStateChanged);
                            info!("'Copy on Exit' checkbox changed to: {}", copier_manager.copy_top_panel_on_exit);
                        }
                        // --- End checkbox interaction ---
                    });
                });
            }
            ui.separator();

            // --- Row 2: Mode Toggles and Add Row ---
            // (This row remains the same)
            ui.horizontal(|ui| {
                let is_sheet_selected = state.selected_sheet_name.is_some();
                if state.ai_mode == AiModeState::Idle {
                    let can_prepare_ai = is_sheet_selected && !state.delete_row_mode_active;
                    if ui.add_enabled(can_prepare_ai, egui::Button::new("✨ Prepare for AI")).on_hover_text("Enable row selection and AI controls").clicked() {
                        state.ai_mode = AiModeState::Preparing;
                        state.ai_selected_rows.clear();
                    }
                } else {
                    if ui.button("❌ Cancel AI Mode").clicked() {
                        state.reset_selection_modes();
                    }
                }
                ui.separator();
                if !state.delete_row_mode_active {
                    let can_delete_row = is_sheet_selected && state.ai_mode == AiModeState::Idle;
                    if ui.add_enabled(can_delete_row, egui::Button::new("🗑️ Delete Row")).on_hover_text("Enable row selection for deletion").clicked() {
                        state.delete_row_mode_active = true;
                        state.ai_selected_rows.clear();
                    }
                } else {
                    if ui.button("❌ Cancel Delete").clicked() {
                        state.delete_row_mode_active = false;
                        state.ai_selected_rows.clear();
                    }
                }
                ui.separator();
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
            });
        });
}

// --- show_delete_row_control_panel function remains the same ---
pub(crate) fn show_delete_row_control_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    mut delete_rows_event_writer: EventWriter<RequestDeleteRows>,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Row Deletion Mode Active").color(egui::Color32::YELLOW).strong());
        ui.separator();

        let is_sheet_selected = state.selected_sheet_name.is_some();
        let can_delete_selected_rows = is_sheet_selected && !state.ai_selected_rows.is_empty();
        let delete_button_text = format!("Delete Selected ({})", state.ai_selected_rows.len());

        if ui.add_enabled(can_delete_selected_rows, egui::Button::new(delete_button_text))
             .on_hover_text("Delete the rows currently selected in the table below")
             .clicked()
        {
            if let Some(sheet_name) = &state.selected_sheet_name {
                delete_rows_event_writer.send(RequestDeleteRows {
                    category: state.selected_category.clone(),
                    sheet_name: sheet_name.clone(),
                    row_indices: state.ai_selected_rows.clone(),
                });
                state.delete_row_mode_active = false; // Automatically exit delete mode
                state.ai_selected_rows.clear();
                state.force_filter_recalculation = true;
            }
        }
    });
}