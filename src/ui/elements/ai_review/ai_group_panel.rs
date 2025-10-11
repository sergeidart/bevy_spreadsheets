// AI Schema Group management UI extracted from monolithic ai_control_panel
use bevy::prelude::*;
use bevy_egui::egui;

use crate::sheets::definitions::SheetMetadata;
use crate::sheets::events::{
    RequestCreateAiSchemaGroup, RequestDeleteAiSchemaGroup, RequestRenameAiSchemaGroup,
    RequestSelectAiSchemaGroup,
};
use crate::ui::elements::editor::state::EditorWindowState;

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_group_panel(
    ui: &mut egui::Ui,
    state: &mut EditorWindowState,
    root_category: &Option<String>,
    root_sheet: &str,
    root_meta: Option<&SheetMetadata>,
    create_group_writer: &mut EventWriter<RequestCreateAiSchemaGroup>,
    rename_group_writer: &mut EventWriter<RequestRenameAiSchemaGroup>,
    select_group_writer: &mut EventWriter<RequestSelectAiSchemaGroup>,
    delete_group_writer: &mut EventWriter<RequestDeleteAiSchemaGroup>,
) {
    let Some(meta) = root_meta else {
        return;
    };
    let groups = meta.ai_schema_groups.clone();
    let active_group = meta.ai_active_schema_group.clone();
    ui.horizontal(|group_ui| {
        let category_for_event = root_category.clone();
        let sheet_for_event = root_sheet.to_string();
        let active_group_name = active_group.clone();

        // Expand / collapse toggle comes first now
        let expanded = state.ai_groups_expanded;
        let toggle_label = if expanded { "<" } else { ">" };
        if group_ui
            .button(toggle_label)
            .on_hover_text(if expanded {
                "Shrink groups row"
            } else {
                "Expand groups row"
            })
            .clicked()
        {
            state.ai_groups_expanded = !expanded;
        }

        if state.ai_groups_expanded {
            group_ui.add_space(6.0);
            // Active group rename/delete icons placed before list for quick access
            if let Some(active_name) = active_group_name.as_ref() {
                if group_ui
                    .button("✏️")
                    .on_hover_text("Rename active group")
                    .clicked()
                {
                    state.ai_group_rename_popup_open = true;
                    state.ai_group_rename_target = Some(active_name.clone());
                    state.ai_group_rename_input = active_name.clone();
                    group_ui.ctx().request_repaint();
                }
                let single_group = groups.len() <= 1;
                let del_btn = group_ui.add_enabled(!single_group, egui::Button::new("🗑️"));
                if single_group {
                    del_btn.on_hover_text("Cannot delete the only remaining group");
                } else if del_btn.on_hover_text("Delete active group").clicked() {
                    state.ai_group_delete_popup_open = true;
                    state.ai_group_delete_target = Some(active_name.clone());
                    state.ai_group_delete_target_category = root_category.clone();
                    state.ai_group_delete_target_sheet = Some(sheet_for_event.clone());
                }
                group_ui.add_space(8.0);
            }
            // Group selection list
            for (idx, group) in groups.iter().enumerate() {
                let is_active = active_group
                    .as_deref()
                    .map(|name| name == group.name.as_str())
                    .unwrap_or(false);
                let response = group_ui.selectable_label(is_active, &group.name);
                if response.clicked() && !is_active && !sheet_for_event.is_empty() {
                    select_group_writer.write(RequestSelectAiSchemaGroup {
                        category: category_for_event.clone(),
                        sheet_name: sheet_for_event.clone(),
                        group_name: group.name.clone(),
                    });
                    state.mark_ai_included_columns_dirty();
                    group_ui.ctx().request_repaint();
                }
                if idx < groups.len() - 1 {
                    group_ui.add_space(6.0);
                }
            }
            group_ui.add_space(4.0);
            let add_button =
                group_ui.add_enabled(!sheet_for_event.is_empty(), egui::Button::new("+ Group"));
            if add_button
                .on_hover_text("Create a new schema group from the current settings")
                .clicked()
            {
                state.ai_group_add_popup_open = true;
                state.ai_group_add_name_input = meta.ensure_unique_schema_group_name("Group");
                group_ui.ctx().request_repaint();
            }

            // (Removed text Ren/Del buttons – replaced with icons above the list)
        }
    });

    // Add Group popup
    if state.ai_group_add_popup_open {
        let mut is_open = true;
        egui::Window::new("New Schema Group")
            .id(egui::Id::new("ai_group_add_popup_window"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
            .open(&mut is_open)
            .show(ui.ctx(), |popup_ui| {
                popup_ui.label("Group name");
                let text_edit = popup_ui.add(
                    egui::TextEdit::singleline(&mut state.ai_group_add_name_input)
                        .hint_text("e.g. Draft")
                        .desired_width(220.0),
                );
                let trimmed = state.ai_group_add_name_input.trim();
                let create_enabled = !trimmed.is_empty() && !root_sheet.is_empty();
                let create_clicked = popup_ui
                    .add_enabled(create_enabled, egui::Button::new("Create"))
                    .clicked();
                let pressed_enter =
                    text_edit.lost_focus() && popup_ui.input(|i| i.key_pressed(egui::Key::Enter));
                let cancel_clicked = popup_ui.button("Cancel").clicked();

                if (create_clicked || (pressed_enter && create_enabled)) && create_enabled {
                    create_group_writer.write(RequestCreateAiSchemaGroup {
                        category: root_category.clone(),
                        sheet_name: root_sheet.to_string(),
                        desired_name: Some(trimmed.to_string()),
                    });
                    state.mark_ai_included_columns_dirty();
                    state.ai_group_add_popup_open = false;
                    state.ai_group_add_name_input.clear();
                    popup_ui.ctx().request_repaint();
                }

                if cancel_clicked {
                    state.ai_group_add_popup_open = false;
                    state.ai_group_add_name_input.clear();
                }
            });
        if !is_open {
            state.ai_group_add_popup_open = false;
            state.ai_group_add_name_input.clear();
        }
    }

    // Rename Group popup
    if state.ai_group_rename_popup_open {
        if let Some(target_name) = state.ai_group_rename_target.clone() {
            let mut is_open = true;
            egui::Window::new("Rename Schema Group")
                .id(egui::Id::new("ai_group_rename_popup_window"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 120.0])
                .open(&mut is_open)
                .show(ui.ctx(), |popup_ui| {
                    popup_ui.label(format!("Rename '{}' to:", target_name));
                    let text_edit = popup_ui.add(
                        egui::TextEdit::singleline(&mut state.ai_group_rename_input)
                            .hint_text("Group name")
                            .desired_width(220.0),
                    );
                    let trimmed = state.ai_group_rename_input.trim();
                    let rename_enabled = !trimmed.is_empty() && !root_sheet.is_empty();
                    let rename_clicked = popup_ui
                        .add_enabled(rename_enabled, egui::Button::new("Rename"))
                        .clicked();
                    let pressed_enter = text_edit.lost_focus()
                        && popup_ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let cancel_clicked = popup_ui.button("Cancel").clicked();

                    if (rename_clicked || (pressed_enter && rename_enabled)) && rename_enabled {
                        rename_group_writer.write(RequestRenameAiSchemaGroup {
                            category: root_category.clone(),
                            sheet_name: root_sheet.to_string(),
                            old_name: target_name.clone(),
                            new_name: trimmed.to_string(),
                        });
                        state.ai_group_rename_popup_open = false;
                        state.ai_group_rename_target = None;
                        state.ai_group_rename_input.clear();
                        popup_ui.ctx().request_repaint();
                    }

                    if cancel_clicked {
                        state.ai_group_rename_popup_open = false;
                        state.ai_group_rename_target = None;
                        state.ai_group_rename_input.clear();
                    }
                });
            if !is_open {
                state.ai_group_rename_popup_open = false;
                state.ai_group_rename_target = None;
                state.ai_group_rename_input.clear();
            }
        } else {
            state.ai_group_rename_popup_open = false;
            state.ai_group_rename_input.clear();
        }
    }

    // Delete confirmation popup
    if state.ai_group_delete_popup_open {
        if let (Some(group_name), Some(cat), Some(sheet)) = (
            state.ai_group_delete_target.clone(),
            state.ai_group_delete_target_category.clone(),
            state.ai_group_delete_target_sheet.clone(),
        ) {
            let mut is_open = true;
            egui::Window::new("Delete Schema Group")
                .id(egui::Id::new("ai_group_delete_confirm"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 160.0])
                .open(&mut is_open)
                .show(ui.ctx(), |popup_ui| {
                    popup_ui.label(format!(
                        "Are you sure you want to delete group '{}' for sheet '{}' ?",
                        group_name, sheet
                    ));
                    popup_ui.add_space(6.0);
                    popup_ui.horizontal(|h| {
                        if h.button("Delete").clicked() {
                            delete_group_writer.write(RequestDeleteAiSchemaGroup {
                                category: Some(cat.clone()),
                                sheet_name: sheet.clone(),
                                group_name: group_name.clone(),
                            });
                            state.ai_group_delete_popup_open = false;
                            state.ai_group_delete_target = None;
                            state.ai_group_delete_target_category = None;
                            state.ai_group_delete_target_sheet = None;
                            h.ctx().request_repaint();
                        }
                        if h.button("Cancel").clicked() {
                            state.ai_group_delete_popup_open = false;
                            state.ai_group_delete_target = None;
                            state.ai_group_delete_target_category = None;
                            state.ai_group_delete_target_sheet = None;
                        }
                    });
                });
            if !is_open {
                state.ai_group_delete_popup_open = false;
                state.ai_group_delete_target = None;
                state.ai_group_delete_target_category = None;
                state.ai_group_delete_target_sheet = None;
            }
        } else {
            state.ai_group_delete_popup_open = false;
            state.ai_group_delete_target = None;
            state.ai_group_delete_target_category = None;
            state.ai_group_delete_target_sheet = None;
        }
    }
}
