// src/ui/elements/popups/structure_recreation_popup.rs
// Popup for handling structure table recreation when table already exists

use crate::sheets::events::{RequestStructureTableRecreation, StructureRecreationStrategy};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use bevy_egui::egui;

/// Shows a popup asking the user how to handle an existing structure table.
/// Options: Careful Recreation (keep data), Clean Start (delete & recreate), or Cancel.
pub fn show_structure_recreation_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    recreation_writer: &mut EventWriter<RequestStructureTableRecreation>,
) {
    if !state.show_structure_recreation_popup {
        return;
    }

    let mut is_open = state.show_structure_recreation_popup;

    egui::Window::new("Structure Table Already Exists")
        .open(&mut is_open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_width(500.0);

            ui.label(format!(
                "The structure table '{}' already exists.",
                state.structure_recreation_sheet_name
            ));
            ui.add_space(10.0);

            ui.label("How would you like to proceed?");
            ui.add_space(15.0);

            // Option 1: Careful Recreation
            ui.horizontal(|ui| {
                ui.label("🔄");
                ui.vertical(|ui| {
                    if ui.button("Careful Recreation").clicked() {
                        if let Some(parent_col_def) = &state.structure_recreation_parent_col_def {
                            recreation_writer.write(RequestStructureTableRecreation {
                                category: state.structure_recreation_category.clone(),
                                structure_sheet_name: state.structure_recreation_sheet_name.clone(),
                                parent_sheet_name: state.structure_recreation_parent_sheet_name.clone(),
                                parent_col_def: parent_col_def.clone(),
                                structure_columns: state.structure_recreation_struct_columns.clone(),
                                strategy: StructureRecreationStrategy::CarefulRecreation,
                            });
                        }
                        state.show_structure_recreation_popup = false;
                    }
                    ui.label("Keep existing data, update schema if needed");
                    ui.label("(Preserves rows, updates column definitions)");
                });
            });
            ui.add_space(10.0);

            // Option 2: Clean Start
            ui.horizontal(|ui| {
                ui.label("🗑️");
                ui.vertical(|ui| {
                    if ui.button("Clean Start").clicked() {
                        if let Some(parent_col_def) = &state.structure_recreation_parent_col_def {
                            recreation_writer.write(RequestStructureTableRecreation {
                                category: state.structure_recreation_category.clone(),
                                structure_sheet_name: state.structure_recreation_sheet_name.clone(),
                                parent_sheet_name: state.structure_recreation_parent_sheet_name.clone(),
                                parent_col_def: parent_col_def.clone(),
                                structure_columns: state.structure_recreation_struct_columns.clone(),
                                strategy: StructureRecreationStrategy::CleanStart,
                            });
                        }
                        state.show_structure_recreation_popup = false;
                    }
                    ui.label("Delete and recreate from scratch");
                    ui.label("(WARNING: All existing data will be lost!)");
                });
            });
            ui.add_space(10.0);

            // Option 3: Cancel
            ui.horizontal(|ui| {
                ui.label("❌");
                ui.vertical(|ui| {
                    if ui.button("Cancel").clicked() {
                        if let Some(parent_col_def) = &state.structure_recreation_parent_col_def {
                            recreation_writer.write(RequestStructureTableRecreation {
                                category: state.structure_recreation_category.clone(),
                                structure_sheet_name: state.structure_recreation_sheet_name.clone(),
                                parent_sheet_name: state.structure_recreation_parent_sheet_name.clone(),
                                parent_col_def: parent_col_def.clone(),
                                structure_columns: state.structure_recreation_struct_columns.clone(),
                                strategy: StructureRecreationStrategy::Cancel,
                            });
                        }
                        state.show_structure_recreation_popup = false;
                    }
                    ui.label("Keep existing table as-is");
                    ui.label("(No changes will be made)");
                });
            });
        });

    if !is_open {
        state.show_structure_recreation_popup = false;
    }
}
