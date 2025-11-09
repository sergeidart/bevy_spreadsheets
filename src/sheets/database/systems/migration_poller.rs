// src/sheets/database/systems/migration_poller.rs

use super::MigrationBackgroundState;
use crate::sheets::events::{MigrationCompleted, MigrationProgress, SheetOperationFeedback};
use crate::sheets::database::daemon_resource::SharedDaemonClient;
use bevy::prelude::*;

/// Poll the background migration thread for progress updates and completion
pub fn poll_migration_background(
    mut bg_state: ResMut<MigrationBackgroundState>,
    mut progress_writer: EventWriter<MigrationProgress>,
    mut completed_writer: EventWriter<MigrationCompleted>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut registry: ResMut<crate::sheets::resources::SheetRegistry>,
    mut data_modified_writer: EventWriter<crate::sheets::events::SheetDataModifiedInRegistryEvent>,
    mut revalidate_writer: EventWriter<crate::sheets::events::RequestSheetRevalidation>,
    mut editor_state: Option<ResMut<crate::ui::elements::editor::state::EditorWindowState>>,
    daemon_client: Res<SharedDaemonClient>,
) {
    // Drain any progress updates
    if let Some(rx) = &bg_state.progress_rx {
        if let Ok(rx) = rx.lock() {
            for msg in rx.try_iter() {
                // For single-file imports, also mirror progress to feedback log instead of popup
                if bg_state.post_select.is_some() {
                    feedback_writer.write(SheetOperationFeedback {
                        message: msg.message.clone(),
                        is_error: false,
                    });
                    info!("{}", msg.message);
                }
                progress_writer.write(msg);
            }
        }
    }

    // Check for completion
    if let Some(rx) = &bg_state.completion_rx {
        let result = rx.lock().ok().and_then(|rx| rx.try_recv().ok());
        if let Some(res) = result {
            match res {
                Ok((report, db_path)) => {
                    let success_msg = format!(
                        "Migration completed! {} sheets migrated, {} failed",
                        report.sheets_migrated, report.sheets_failed
                    );
                    info!("{}", success_msg);
                    if !report.failed_sheets.is_empty() {
                        for (name, err) in report.failed_sheets {
                            warn!("Failed to migrate '{}': {}", name, err);
                        }
                    }
                    feedback_writer.write(SheetOperationFeedback {
                        message: success_msg.clone(),
                        is_error: false,
                    });

                    // Load tables into registry
                    match rusqlite::Connection::open(&db_path) {
                        Ok(conn) => {
                            match crate::sheets::database::reader::DbReader::list_sheets(&conn) {
                                Ok(table_names) => {
                                    let db_name = db_path
                                        .file_stem()
                                        .map(|s| s.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "Unknown".to_string());
                                    for table_name in table_names.iter() {
                                        match crate::sheets::database::reader::DbReader::read_metadata(&conn, table_name, daemon_client.client()) {
                                    Ok(mut metadata) => {
                                        metadata.category = Some(db_name.clone());
                                        match crate::sheets::database::reader::DbReader::read_grid_data(&conn, table_name, &metadata) {
                                            Ok((grid, row_indices)) => {
                                                let sheet_data = crate::sheets::definitions::SheetGridData { grid, metadata: Some(metadata.clone()), row_indices };
                                                registry.add_or_replace_sheet(metadata.category.clone(), table_name.clone(), sheet_data);
                                                data_modified_writer.write(crate::sheets::events::SheetDataModifiedInRegistryEvent { category: Some(db_name.clone()), sheet_name: table_name.clone() });
                                                revalidate_writer.write(crate::sheets::events::RequestSheetRevalidation { category: Some(db_name.clone()), sheet_name: table_name.clone() });
                                            }
                                            Err(e) => error!("Post-migration: Failed to read grid for '{}': {}", table_name, e),
                                        }
                                    }
                                    Err(e) => error!("Post-migration: Failed to read metadata for '{}': {}", table_name, e),
                                }
                                    }
                                    // Auto-select preferred table if provided; otherwise first table
                                    let mut selected_table: Option<String> = None;
                                    if let Some((cat, name)) = bg_state.post_select.take() {
                                        if cat == db_name && table_names.iter().any(|t| t == &name)
                                        {
                                            selected_table = Some(name);
                                        }
                                    }
                                    if selected_table.is_none() {
                                        selected_table = table_names.first().cloned();
                                    }
                                    if let (Some(sel_table), Some(state)) =
                                        (selected_table, editor_state.as_deref_mut())
                                    {
                                        state.selected_category = Some(db_name.clone());
                                        state.selected_sheet_name = Some(sel_table.clone());
                                        state.reset_interaction_modes_and_selections();
                                        state.force_filter_recalculation = true;
                                        info!(
                                            "Auto-selected migrated DB table '{:?}/{}'",
                                            state.selected_category, sel_table
                                        );
                                    }
                                }
                                Err(e) => error!("Post-migration: Failed to list tables: {}", e),
                            }
                        }
                        Err(e) => error!(
                            "Post-migration: Failed to open DB '{}': {}",
                            db_path.display(),
                            e
                        ),
                    }

                    completed_writer.write(MigrationCompleted {});
                    bg_state.progress_rx = None;
                    bg_state.completion_rx = None;
                }
                Err(err) => {
                    let error_msg = format!("Migration failed: {}", err);
                    error!("{}", error_msg);
                    feedback_writer.write(SheetOperationFeedback {
                        message: error_msg.clone(),
                        is_error: true,
                    });
                    completed_writer.write(MigrationCompleted {});
                    bg_state.progress_rx = None;
                    bg_state.completion_rx = None;
                }
            }
        }
    }
}
