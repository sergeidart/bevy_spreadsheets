// src/ui/elements/top_panel.rs
use crate::sheets::events::{
    AddSheetRowRequest, RequestDeleteRows, RequestInitiateFileUpload,
    RequestDeleteColumns,
    RequestAddColumn,
};
use crate::sheets::resources::SheetRegistry;
use crate::ui::elements::editor::state::{AiModeState, EditorWindowState, SheetInteractionState};
use bevy::prelude::*;
use bevy_egui::egui;

use crate::visual_copier::{
    resources::VisualCopierManager,
    events::{
        PickFolderRequest, QueueTopPanelCopyEvent, ReverseTopPanelFoldersEvent,
        VisualCopierStateChanged,
        RequestAppExit,
    },
};

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
    mut add_column_event_writer: EventWriter<RequestAddColumn>,
    mut upload_req_writer: EventWriter<RequestInitiateFileUpload>,
    mut copier_manager: ResMut<VisualCopierManager>,
    mut pick_folder_writer: EventWriter<PickFolderRequest>,
    mut queue_top_panel_copy_writer: EventWriter<QueueTopPanelCopyEvent>,
    mut reverse_folders_writer: EventWriter<ReverseTopPanelFoldersEvent>,
    mut request_app_exit_writer: EventWriter<RequestAppExit>,
    mut state_changed_writer: EventWriter<VisualCopierStateChanged>,
) {
    egui::TopBottomPanel::top("main_top_controls_panel")
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
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
                                state.reset_interaction_modes_and_selections();
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
                                        state.reset_interaction_modes_and_selections();
                                        state.force_filter_recalculation = true;
                                        state.ai_rule_popup_needs_init = true;
                                    }
                                }
                            }
                        }
                    });

                if category_response.response.changed() { 
                    if state.selected_sheet_name.is_none() { 
                         state.reset_interaction_modes_and_selections();
                    }
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
                                state.reset_interaction_modes_and_selections();
                                state.force_filter_recalculation = true;
                                state.ai_rule_popup_needs_init = true;
                            }
                        });
                    if sheet_response.response.changed() {
                         if state.selected_sheet_name.is_none() {
                             state.reset_interaction_modes_and_selections();
                         }
                         state.force_filter_recalculation = true;
                         state.ai_rule_popup_needs_init = true;
                    }
                    if let Some(current_sheet_name) = state.selected_sheet_name.as_ref() {
                        if !registry.get_sheet_names_in_category(&state.selected_category).contains(current_sheet_name) {
                            state.selected_sheet_name = None;
                            state.reset_interaction_modes_and_selections();
                            state.force_filter_recalculation = true;
                            state.ai_rule_popup_needs_init = true;
                        }
                    }
                });

                let is_sheet_selected = state.selected_sheet_name.is_some();
                let can_manage_sheet = is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;

                if ui.add_enabled(can_manage_sheet, egui::Button::new("✏ Rename")).clicked() {
                    if let Some(ref name_to_rename) = state.selected_sheet_name {
                        state.rename_target_category = state.selected_category.clone();
                        state.rename_target_sheet = name_to_rename.clone();
                        state.new_name_input = state.rename_target_sheet.clone();
                        state.show_rename_popup = true;
                    }
                }
                if ui.add_enabled(can_manage_sheet, egui::Button::new("🗑 Delete Sheet")).clicked() {
                    if let Some(ref name_to_delete) = state.selected_sheet_name {
                        state.delete_target_category = state.selected_category.clone();
                        state.delete_target_sheet = name_to_delete.clone();
                        state.show_delete_confirm_popup = true;
                    }
                }

                let copy_button_text = if state.show_quick_copy_bar { "❌ Close Copy" } else { "📋 Copy" };
                if ui.button(copy_button_text).clicked() {
                    state.show_quick_copy_bar = !state.show_quick_copy_bar;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new("❌ App Exit")).clicked() {
                        info!("'App Exit' button clicked. Sending RequestAppExit event.");
                        request_app_exit_writer.send(RequestAppExit);
                    }
                });
            });

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
                        let checkbox_response = ui.checkbox(&mut copier_manager.copy_top_panel_on_exit, "Copy on Exit")
                            .on_hover_text("If checked, performs this Quick Copy operation synchronously just before the application closes via 'App Exit'.");
                        if checkbox_response.changed() {
                            state_changed_writer.send(VisualCopierStateChanged);
                            info!("'Copy on Exit' checkbox changed to: {}", copier_manager.copy_top_panel_on_exit);
                        }
                    });
                });
            }
            ui.separator();

            ui.horizontal(|ui| {
                let is_sheet_selected = state.selected_sheet_name.is_some();

                let can_add_row = is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;
                if ui.add_enabled(can_add_row, egui::Button::new("➕ Add Row")).clicked() {
                    if let Some(sheet_name) = &state.selected_sheet_name {
                        add_row_event_writer.send(AddSheetRowRequest {
                            category: state.selected_category.clone(),
                            sheet_name: sheet_name.clone(),
                        });
                        // MODIFIED: Use the renamed state flag
                        state.request_scroll_to_new_row = true;
                        state.force_filter_recalculation = true; 
                    }
                }
                ui.separator();

                if state.current_interaction_mode == SheetInteractionState::AiModeActive {
                    if ui.button("❌ Cancel AI").clicked() {
                        state.reset_interaction_modes_and_selections();
                    }
                } else {
                    let can_enter_ai_mode = is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;
                    if ui.add_enabled(can_enter_ai_mode, egui::Button::new("✨ AI Mode")).on_hover_text("Enable row selection and AI controls").clicked() {
                        state.current_interaction_mode = SheetInteractionState::AiModeActive;
                        state.ai_mode = AiModeState::Preparing; 
                        state.ai_selected_rows.clear();
                    }
                }
                ui.separator();

                if state.current_interaction_mode == SheetInteractionState::DeleteModeActive {
                    if ui.button("❌ Cancel Delete").clicked() {
                        state.reset_interaction_modes_and_selections();
                    }
                } else {
                    let can_enter_delete_mode = is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;
                    if ui.add_enabled(can_enter_delete_mode, egui::Button::new("🗑️ Delete Mode")).on_hover_text("Enable row and column selection for deletion").clicked() {
                        state.current_interaction_mode = SheetInteractionState::DeleteModeActive;
                        state.ai_selected_rows.clear(); 
                        state.selected_columns_for_deletion.clear();
                    }
                }
                ui.separator();

                if state.current_interaction_mode == SheetInteractionState::ColumnModeActive {
                     if ui.button("➕ Add Column").on_hover_text("Add a new column to the current sheet").clicked(){
                         if let Some(sheet_name) = &state.selected_sheet_name {
                            add_column_event_writer.send(RequestAddColumn {
                                category: state.selected_category.clone(),
                                sheet_name: sheet_name.clone(),
                            });
                         }
                     }
                     if ui.button("❌ Finish Column Edit").clicked() { 
                        state.reset_interaction_modes_and_selections();
                    }
                } else {
                    let can_enter_column_mode = is_sheet_selected && state.current_interaction_mode == SheetInteractionState::Idle;
                    if ui.add_enabled(can_enter_column_mode, egui::Button::new("🏛️ Column Mode")).on_hover_text("Enable column adding, deletion, and reordering").clicked() {
                        state.current_interaction_mode = SheetInteractionState::ColumnModeActive;
                    }
                }
            });
            ui.add_space(5.0);
        });
}

pub(crate) fn show_delete_mode_control_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    mut delete_rows_event_writer: EventWriter<RequestDeleteRows>,
    mut delete_columns_event_writer: EventWriter<RequestDeleteColumns>,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Delete Mode Active: Select rows and/or columns to delete.").color(egui::Color32::YELLOW).strong());
        ui.separator();

        let is_sheet_selected = state.selected_sheet_name.is_some();
        
        let rows_selected_count = state.ai_selected_rows.len();
        let cols_selected_count = state.selected_columns_for_deletion.len();

        let can_delete_anything = is_sheet_selected && (rows_selected_count > 0 || cols_selected_count > 0);
        
        let mut button_text = "Delete Selected".to_string();
        if rows_selected_count > 0 && cols_selected_count > 0 {
            button_text = format!("Delete ({} Rows, {} Cols)", rows_selected_count, cols_selected_count);
        } else if rows_selected_count > 0 {
            button_text = format!("Delete {} Row(s)", rows_selected_count);
        } else if cols_selected_count > 0 {
            button_text = format!("Delete {} Col(s)", cols_selected_count);
        }

        if ui.add_enabled(can_delete_anything, egui::Button::new(button_text))
             .on_hover_text("Delete the selected rows and/or columns from the table")
             .clicked()
        {
            if let Some(sheet_name) = &state.selected_sheet_name {
                let mut actions_taken = false;
                if rows_selected_count > 0 {
                    delete_rows_event_writer.send(RequestDeleteRows {
                        category: state.selected_category.clone(),
                        sheet_name: sheet_name.clone(),
                        row_indices: state.ai_selected_rows.clone(),
                    });
                    actions_taken = true;
                }
                if cols_selected_count > 0 {
                    delete_columns_event_writer.send(RequestDeleteColumns {
                        category: state.selected_category.clone(),
                        sheet_name: sheet_name.clone(),
                        column_indices: state.selected_columns_for_deletion.clone(),
                    });
                    actions_taken = true;
                }

                if actions_taken {
                    state.reset_interaction_modes_and_selections(); 
                    state.force_filter_recalculation = true;
                }
            }
        }
    });
}