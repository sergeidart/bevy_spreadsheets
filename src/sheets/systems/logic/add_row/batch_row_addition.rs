// src/sheets/systems/logic/add_row/batch_row_addition.rs
// Batch row addition handler - adds multiple rows with single row_index calculation

use crate::sheets::{
    events::{AddSheetRowsBatchRequest, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;

use super::{
    cache_handlers::{get_structure_context, invalidate_sheet_cache, resolve_virtual_context},
    db_persistence::persist_rows_batch_to_db,
};

/// Batch handler for add row requests - adds multiple rows at once
/// Prevents race conditions by calculating row_index once for all rows
pub fn handle_add_rows_batch_request(
    mut events: EventReader<AddSheetRowsBatchRequest>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut editor_state: Option<ResMut<EditorWindowState>>,
) {
    for event in events.read() {
        if event.rows_initial_values.is_empty() {
            continue;
        }

        // Resolve virtual context if active
        let (category, sheet_name) = resolve_virtual_context(
            &editor_state,
            event.category.clone(),
            event.sheet_name.clone(),
        );

        // Get structure context (parent_key) if in structure navigation
        let structure_context = get_structure_context(&editor_state, &sheet_name, &category, &registry);

        let num_rows = event.rows_initial_values.len();
        let mut metadata_cache: Option<crate::sheets::definitions::SheetMetadata> = None;

        if let Some(sheet_data) = registry.get_sheet_mut(&category, &sheet_name) {
            if let Some(metadata) = &sheet_data.metadata {
                let num_cols = metadata.columns.len();
                
                // Detect if this is a structure sheet
                let is_structure_sheet = num_cols >= 2
                    && metadata
                        .columns
                        .get(0)
                        .map(|c| c.header.eq_ignore_ascii_case("row_index"))
                        .unwrap_or(false)
                    && metadata
                        .columns
                        .get(1)
                        .map(|c| c.header.eq_ignore_ascii_case("parent_key"))
                        .unwrap_or(false);

                // Insert all rows at top (index 0, 1, 2, etc.)
                for (row_idx, initial_values) in event.rows_initial_values.iter().enumerate() {
                    sheet_data.grid.insert(row_idx, vec![String::new(); num_cols]);

                    // Auto-fill structure sheet columns
                    if is_structure_sheet {
                        if let Some(row) = sheet_data.grid.get_mut(row_idx) {
                            // Auto-fill row_index column (index 0)
                            if row.len() > 0 && row[0].is_empty() {
                                row[0] = row_idx.to_string();
                            }
                            // Auto-fill parent_key column (index 1)
                            if row.len() > 1 && row[1].is_empty() {
                                if let Some(parent_key) = &structure_context {
                                    row[1] = parent_key.clone();
                                }
                            }
                        }
                    }

                    // Apply initial values
                    if let Some(row) = sheet_data.grid.get_mut(row_idx) {
                        for (col, val) in initial_values {
                            if *col < row.len() {
                                row[*col] = val.clone();
                            }
                        }
                    }
                }

                let msg = format!(
                    "Added {} new row(s) at the top of sheet '{:?}/{}'.",
                    num_rows, category, sheet_name
                );
                info!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: false,
                });

                // Invalidate cache
                invalidate_sheet_cache(&mut editor_state, &category, &sheet_name);

                metadata_cache = Some(metadata.clone());

                // Persist to DB if DB-backed
                if let Some(meta) = &sheet_data.metadata {
                    if meta.category.is_some() {
                        // DB-backed: batch insert all rows
                        let persist_start = std::time::Instant::now();
                        match persist_rows_batch_to_db(meta, &sheet_name, &category, &sheet_data.grid, num_rows) {
                            Ok(_) => {
                                let duration = persist_start.elapsed();
                                info!("Batch of {} rows persisted to DB in {:?}", num_rows, duration);
                                
                                // For structure sheets, reload row_index values from DB
                                let is_structure_sheet = meta.columns.len() >= 2
                                    && meta.columns.get(0).map(|c| c.header.eq_ignore_ascii_case("row_index")).unwrap_or(false);
                                
                                if is_structure_sheet {
                                    // Query the top N rows from DB to get actual row_index values
                                    if let Some(cat) = &meta.category {
                                        let base_path = crate::sheets::systems::io::get_default_data_base_path();
                                        let db_path = base_path.join(format!("{}.db", cat));
                                        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                                            // Get row_index values for the top N rows (ORDER BY row_index DESC)
                                            let query = format!(
                                                "SELECT row_index FROM \"{}\" ORDER BY row_index DESC LIMIT {}",
                                                sheet_name, num_rows
                                            );
                                            if let Ok(mut stmt) = conn.prepare(&query) {
                                                let row_indices_result: Result<Vec<i64>, rusqlite::Error> = stmt
                                                    .query_map([], |row| row.get(0))
                                                    .and_then(|mapped| mapped.collect());
                                                if let Ok(indices) = row_indices_result {
                                                    info!("Reloaded row_index values from DB: {:?}", indices);
                                                    // Update grid with actual row_index values
                                                    for (grid_idx, &db_row_index) in indices.iter().enumerate() {
                                                        if let Some(row) = sheet_data.grid.get_mut(grid_idx) {
                                                            if !row.is_empty() {
                                                                row[0] = db_row_index.to_string();
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    warn!("Failed to reload row_index values from DB");
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to persist batch rows to DB: {}", e);
                            }
                        }
                    } else {
                        // JSON sheets not supported for batch yet - they should use individual AddSheetRowRequest
                        warn!("Batch add not supported for JSON sheets yet");
                    }
                }

                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });
            } else {
                let msg = format!(
                    "Cannot add rows to sheet '{:?}/{}': Metadata missing.",
                    category, sheet_name
                );
                warn!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        } else {
            let msg = format!(
                "Cannot add rows: Sheet '{:?}/{}' not found in registry.",
                category, sheet_name
            );
            warn!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
        }
    }
}
