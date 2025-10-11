// src/ui/elements/popups/migration_popup.rs

use bevy::prelude::*;
use bevy_egui::egui;
use std::path::PathBuf;

use crate::sheets::database::DbConfig;
use crate::sheets::events::{RequestMigrateJsonToDb, SheetOperationFeedback};

#[derive(Resource, Default)]
pub struct MigrationPopupState {
    pub show: bool,
    pub source_folder: Option<PathBuf>,
    pub target_db: Option<PathBuf>,
    pub create_new_db: bool,
    pub db_name_input: String,
    pub migration_in_progress: bool,
    pub progress_total: usize,
    pub progress_completed: usize,
    pub progress_message: String,
}

pub fn show_migration_popup(
    ui: &mut egui::Ui,
    state: &mut MigrationPopupState,
    migration_events: &mut EventWriter<RequestMigrateJsonToDb>,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
) {
    if !state.show {
        return;
    }

    let mut open = state.show;
    egui::Window::new("Migrate JSON to Database")
        .open(&mut open)
        .resizable(true)
        .default_width(500.0)
        .show(ui.ctx(), |ui| {
            ui.heading("Import JSON Sheets to SQLite Database");
            ui.add_space(10.0);

            // Source folder selection
            ui.horizontal(|ui| {
                ui.label("Source Folder:");
                if ui.button("📁 Browse...").clicked() {
                    if let Some(folder) = rfd::FileDialog::new()
                        .set_title("Select Folder with JSON Sheets")
                        .pick_folder()
                    {
                        state.source_folder = Some(folder);
                    }
                }
            });

            if let Some(folder) = &state.source_folder {
                ui.label(format!("Selected: {}", folder.display()));
            } else {
                ui.colored_label(egui::Color32::GRAY, "No folder selected");
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // Database selection
            ui.label("Target Database:");

            ui.horizontal(|ui| {
                ui.radio_value(&mut state.create_new_db, false, "Add to existing DB");
                ui.radio_value(&mut state.create_new_db, true, "Create new DB");
            });

            ui.add_space(5.0);

            if state.create_new_db {
                // New database creation
                ui.horizontal(|ui| {
                    ui.label("Database name:");
                    ui.text_edit_singleline(&mut state.db_name_input);
                    if !state.db_name_input.ends_with(".db") {
                        ui.label(".db");
                    }
                });

                let skyline_path = DbConfig::default_path();
                let proposed_path = skyline_path.join(if state.db_name_input.ends_with(".db") {
                    state.db_name_input.clone()
                } else {
                    format!("{}.db", state.db_name_input)
                });

                ui.label(format!("Will create: {}", proposed_path.display()));
                state.target_db = Some(proposed_path);
            } else {
                // Select existing database
                ui.horizontal(|ui| {
                    ui.label("Select DB:");
                    if ui.button("📁 Browse...").clicked() {
                        if let Some(db_file) = rfd::FileDialog::new()
                            .set_title("Select SQLite Database")
                            .add_filter("SQLite Database", &["db"])
                            .set_directory(DbConfig::default_path())
                            .pick_file()
                        {
                            state.target_db = Some(db_file);
                        }
                    }
                });

                if let Some(db) = &state.target_db {
                    ui.label(format!("Selected: {}", db.display()));
                } else {
                    ui.colored_label(egui::Color32::GRAY, "No database selected");
                }
            }

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // Info section
            if let Some(folder) = &state.source_folder {
                ui.label("📋 Migration will:");
                ui.add_space(5.0);
                ui.label("  • Scan folder for .json and .meta.json pairs");
                ui.label("  • Detect linked sheet dependencies");
                ui.label("  • Suggest migrating related sheets");
                ui.label("  • Create tables with proper foreign keys");
                ui.label("  • Preserve all metadata and AI settings");
                ui.add_space(5.0);

                // Preview scan
                match crate::sheets::database::MigrationTools::scan_json_folder(folder) {
                    Ok(sheets) => {
                        ui.label(format!(
                            "✅ Found {} sheet(s) ready to migrate",
                            sheets.len()
                        ));

                        if !sheets.is_empty() {
                            if ui.small_button("Show details...").clicked() {
                                // TODO: Show detailed list
                            }
                        }
                    }
                    Err(e) => {
                        ui.colored_label(
                            egui::Color32::RED,
                            format!("⚠ Error scanning folder: {}", e),
                        );
                    }
                }
            }

            ui.add_space(15.0);

            // Action buttons
            ui.horizontal(|ui| {
                let can_migrate = state.source_folder.is_some()
                    && state.target_db.is_some()
                    && !state.migration_in_progress;

                if state.migration_in_progress {
                    ui.spinner();
                    let total = state.progress_total;
                    let completed = state.progress_completed.min(total);
                    let ratio = if total > 0 {
                        completed as f32 / total as f32
                    } else {
                        0.0
                    };
                    if total > 0 {
                        ui.add(
                            egui::ProgressBar::new(ratio)
                                .show_percentage()
                                .text(state.progress_message.clone()),
                        );
                    } else {
                        ui.label(state.progress_message.clone());
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        state.show = false;
                    }

                    ui.add_enabled_ui(can_migrate, |ui| {
                        if ui.button("🚀 Start Migration").clicked() {
                            if let (Some(folder), Some(db)) =
                                (&state.source_folder, &state.target_db)
                            {
                                migration_events.write(RequestMigrateJsonToDb {
                                    json_folder_path: folder.clone(),
                                    target_db_path: db.clone(),
                                    create_new_db: state.create_new_db,
                                });

                                state.migration_in_progress = true;
                                state.progress_total = 0;
                                state.progress_completed = 0;
                                state.progress_message = "Starting migration...".into();

                                feedback_writer.write(SheetOperationFeedback {
                                    message: "Migration started...".to_string(),
                                    is_error: false,
                                });
                            }
                        }
                    });
                });
            });
        });

    state.show = open;
}

// System helper: update popup state from progress events
pub fn update_migration_progress_ui(
    mut events: EventReader<crate::sheets::events::MigrationProgress>,
    mut state: ResMut<MigrationPopupState>,
) {
    for ev in events.read() {
        state.progress_total = ev.total;
        state.progress_completed = ev.completed;
        // Always show the detailed message so per-1k row updates are visible
        state.progress_message = ev.message.clone();
    }
}
