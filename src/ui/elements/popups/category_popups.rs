// src/ui/elements/popups/category_popups.rs
use crate::sheets::events::{RequestCreateCategory, RequestDeleteCategory};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use bevy_egui::egui;

pub fn show_new_category_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    create_category_writer: &mut EventWriter<RequestCreateCategory>,
) {
    if !state.show_new_category_popup {
        return;
    }
    let mut open = state.show_new_category_popup;
    let mut create_clicked = false;
    let mut cancel_clicked = false;

    egui::Window::new("Create New Category")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Enter a category (folder) name:");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut state.new_category_name_input).desired_width(220.0),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                create_clicked = true;
            }
            ui.separator();
            ui.horizontal(|ui_h| {
                if ui_h
                    .add_enabled(
                        !state.new_category_name_input.trim().is_empty(),
                        egui::Button::new("Create"),
                    )
                    .clicked()
                {
                    create_clicked = true;
                }
                if ui_h.button("Cancel").clicked() {
                    cancel_clicked = true;
                }
            });
        });

    if create_clicked {
        let name = state.new_category_name_input.trim();
        if !name.is_empty() {
            create_category_writer.write(RequestCreateCategory {
                name: name.to_string(),
            });
            // Immediately switch selection to the new category
            state.selected_category = Some(name.to_string());
            state.selected_sheet_name = None;
            state.reset_interaction_modes_and_selections();
            state.force_filter_recalculation = true;
            state.show_new_category_popup = false;
            state.new_category_name_input.clear();
        }
    }
    if cancel_clicked || !open {
        state.show_new_category_popup = false;
        state.new_category_name_input.clear();
    }
}

pub fn show_delete_category_confirm_popups(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    delete_category_writer: &mut EventWriter<RequestDeleteCategory>,
) {
    // First confirmation
    if state.show_delete_category_confirm_popup {
        let mut open = state.show_delete_category_confirm_popup;
        let mut proceed = false;
        let mut cancel = false;
        let name = state.delete_category_name.clone().unwrap_or_default();
        egui::Window::new("Delete Category?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "You're about to delete the category '{}' and all its sheets.",
                        name
                    ),
                );
                ui.label("This will remove:");
                ui.label(" • All sheets under this category from the registry");
                ui.label(" • All associated JSON files on disk (grid and metadata)");
                ui.label(" • Any nested structure references within those sheets");
                ui.separator();
                ui.horizontal(|ui_h| {
                    if ui_h
                        .add(egui::Button::new("Proceed").fill(egui::Color32::DARK_RED))
                        .clicked()
                    {
                        proceed = true;
                    }
                    if ui_h.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if cancel || !open {
            state.show_delete_category_confirm_popup = false;
        }
        if proceed {
            state.show_delete_category_confirm_popup = false;
            state.show_delete_category_double_confirm_popup = true;
        }
    }

    // Second confirmation (Really Sure?)
    if state.show_delete_category_double_confirm_popup {
        let mut open = state.show_delete_category_double_confirm_popup;
        let mut really_delete = false;
        let mut cancel = false;
        let name = state.delete_category_name.clone().unwrap_or_default();
        egui::Window::new("Really delete category?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.colored_label(
                    egui::Color32::RED,
                    format!(
                        "This is permanent: '{}' and all its sheets will be deleted.",
                        name
                    ),
                );
                ui.label("Are you REALLY sure?");
                ui.separator();
                ui.horizontal(|ui_h| {
                    if ui_h
                        .add(egui::Button::new("YES, DELETE ALL").fill(egui::Color32::RED))
                        .clicked()
                    {
                        really_delete = true;
                    }
                    if ui_h.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if really_delete {
            if let Some(name) = state.delete_category_name.clone() {
                delete_category_writer.write(RequestDeleteCategory { name });
            }
            // Clear selection if current category was deleted
            if let Some(sel) = state.selected_category.clone() {
                if state.delete_category_name.as_deref() == Some(sel.as_str()) {
                    state.selected_category = None;
                    state.selected_sheet_name = None;
                }
            }
            state.show_delete_category_double_confirm_popup = false;
            state.delete_category_name = None;
        }
        if cancel || !open {
            state.show_delete_category_double_confirm_popup = false;
            // Keep name but cancel the full flow
            state.delete_category_name = None;
        }
    }
}
