use bevy::prelude::*;
use bevy_egui::egui;

use crate::ui::elements::editor::state::EditorWindowState;
use crate::ui::elements::popups::MigrationPopupState;

/// Renders the "Add Table" popup for database mode.
/// 
/// This popup allows the user to:
/// 1. Enter a name for a new table (manual creation - TODO)
/// 2. Click "Migrate from JSON" to open the migration popup
pub fn show_add_table_popup(
    ctx: &egui::Context,
    state: &mut EditorWindowState,
    migration_state: &mut MigrationPopupState,
) {
    if !state.show_add_table_popup {
        return;
    }

    let mut open = true;
    egui::Window::new("📊 Add Table")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 12.0;

                // Instructions
                ui.label("Create a new table in the database:");
                ui.add_space(8.0);

                // Table name input
                ui.horizontal(|ui| {
                    ui.label("Table Name:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_sheet_name_input)
                            .desired_width(200.0)
                            .hint_text("Enter table name..."),
                    );
                });
                ui.add_space(8.0);

                // Create button (manual creation - not yet implemented)
                ui.horizontal(|ui| {
                    if ui
                        .button("✅ Create Empty Table")
                        .on_hover_text("Create a new empty table with the given name")
                        .clicked()
                    {
                        // TODO: Implement manual table creation via event
                        info!("Manual table creation not yet implemented: {}", state.new_sheet_name_input);
                    }

                    // Migrate from JSON button
                    if ui
                        .button("📂 Migrate from JSON")
                        .on_hover_text("Import tables from JSON files into the database")
                        .clicked()
                    {
                        // Close this popup and open migration popup
                        state.show_add_table_popup = false;
                        migration_state.show = true;
                        info!("Opening migration popup...");
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Cancel button
                if ui.button("❌ Cancel").clicked() {
                    state.show_add_table_popup = false;
                }
            });
        });

    if !open {
        state.show_add_table_popup = false;
    }
}
