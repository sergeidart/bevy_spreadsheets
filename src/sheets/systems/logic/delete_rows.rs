// src/sheets/systems/logic/delete_rows.rs
use crate::sheets::{
    definitions::SheetMetadata, // Need metadata for saving
    events::{RequestDeleteRows, SheetDataModifiedInRegistryEvent, SheetOperationFeedback}, // Added SheetDataModifiedInRegistryEvent
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use std::collections::HashMap;

/// Handles deleting one or more specified rows from a sheet.
pub fn handle_delete_rows_request(
    mut events: EventReader<RequestDeleteRows>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>, // Added writer
    editor_state: Option<Res<crate::ui::elements::editor::state::EditorWindowState>>, // To map virtual sheets to parent contexts
) {
    // Use map to track sheets needing save after deletions
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category;
        let sheet_name = &event.sheet_name;
        let indices_to_delete = &event.row_indices;

        if indices_to_delete.is_empty() {
            trace!(
                "Skipping delete request for '{:?}/{}': No indices provided.",
                category,
                sheet_name
            );
            continue;
        }

        // Check if this is a virtual structure sheet
        let mut is_virtual = false;
        let mut parent_ctx_opt = None;
        if let Some(state) = editor_state.as_ref() {
            if sheet_name.starts_with("__virtual__") {
                // Find corresponding context
                if let Some(vctx) = state
                    .virtual_structure_stack
                    .iter()
                    .find(|v| &v.virtual_sheet_name == sheet_name)
                {
                    is_virtual = true;
                    parent_ctx_opt = Some(vctx.parent.clone());
                }
            }
        }

        // --- Perform Deletion (Mutable Borrow) ---
        let mut operation_successful = false;
        let mut deleted_count = 0;
        let mut error_message: Option<String> = None;
        let mut metadata_cache: Option<SheetMetadata> = None;

        // Pre-capture info for DB deletion using row_index values directly from grid
        let mut db_backed: Option<(
            String,      /*db_name*/
            bool,        /*is_structure_table*/
            Vec<i64>,    /*row_index values to delete*/
        )> = None;
        if let Some(sheet_ro) = registry.get_sheet(category, sheet_name) {
            info!("delete_rows: Got sheet '{:?}/{}' from registry: grid.len()={}, row_indices.len()={}",
                  category, sheet_name, sheet_ro.grid.len(), sheet_ro.row_indices.len());
            if let Some(meta) = &sheet_ro.metadata {
                if let Some(db_name) = &meta.category {
                    // Detect structure table by checking if row_index is at position 0
                    // Structure tables have: row_index (0, hidden), parent_key (1, visible), then user columns
                    // Regular tables have: row_index (hidden, not in columns list), then user columns
                    let is_structure = meta.columns.len() >= 2
                        && meta
                            .columns
                            .get(0)
                            .map(|c| c.header.eq_ignore_ascii_case("row_index"))
                            .unwrap_or(false)
                        && meta
                            .columns
                            .get(1)
                            .map(|c| c.header.eq_ignore_ascii_case("parent_key"))
                            .unwrap_or(false);
                    
                    info!("Sheet '{}' is_structure={}, columns={:?}", 
                          sheet_name, is_structure, 
                          meta.columns.iter().take(3).map(|c| &c.header).collect::<Vec<_>>());
                    
                    // Extract row_index values from the grid BEFORE any deletion
                    let mut row_index_values: Vec<i64> = Vec::new();
                    
                    if is_structure {
                        // For structure tables, row_index is in column 0
                        for &idx in indices_to_delete.iter() {
                            if let Some(row) = sheet_ro.grid.get(idx) {
                                // Column 0 is row_index for structure sheets
                                if let Some(row_index_str) = row.get(0) {
                                    if let Ok(row_index_val) = row_index_str.trim().parse::<i64>() {
                                        row_index_values.push(row_index_val);
                                        info!("Structure sheet: Grid idx {} -> row_index {} in '{}'", 
                                              idx, row_index_val, sheet_name);
                                    }
                                }
                            }
                        }
                    } else {
                        // For regular tables, use the row_indices field which maps grid index to row_index
                        for &grid_idx in indices_to_delete.iter() {
                            if let Some(&row_index_val) = sheet_ro.row_indices.get(grid_idx) {
                                row_index_values.push(row_index_val);
                                info!("Regular table: Grid idx {} -> row_index {} in '{}'", 
                                      grid_idx, row_index_val, sheet_name);
                            } else {
                                error!(
                                    "Could not find row_index for grid index {} in '{:?}/{}' (row_indices len: {})",
                                    grid_idx, category, sheet_name, sheet_ro.row_indices.len()
                                );
                            }
                        }
                    }
                    db_backed = Some((db_name.clone(), is_structure, row_index_values));
                }
            }
        }

        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            // Sort indices descending to avoid index shifting issues during removal
            let mut sorted_indices: Vec<usize> = indices_to_delete.iter().cloned().collect();
            sorted_indices.sort_unstable_by(|a, b| b.cmp(a)); // Sort descending

            let initial_row_count = sheet_data.grid.len();

            for &index in &sorted_indices {
                if index < sheet_data.grid.len() {
                    sheet_data.grid.remove(index);
                    // Also remove corresponding row_index if it exists
                    if index < sheet_data.row_indices.len() {
                        sheet_data.row_indices.remove(index);
                    }
                    deleted_count += 1;
                } else {
                    error_message = Some(format!(
                        "Index {} out of bounds ({} rows). Deletion partially failed.",
                        index, initial_row_count
                    ));
                    // Stop processing further indices for this event on error? Or just skip?
                    // Let's just skip the invalid index and report the partial failure.
                    warn!(
                        "Skipping delete for index {} in '{:?}/{}': Out of bounds.",
                        index, category, sheet_name
                    );
                }
            }

            if deleted_count > 0 {
                operation_successful = true; // Mark successful even if partial
                                             // Cache metadata for saving if deletion occurred
                metadata_cache = sheet_data.metadata.clone();
                // Send data modified event
                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });
            }
        } else {
            error_message = Some(format!("Sheet '{:?}/{}' not found.", category, sheet_name));
        }

        // --- Feedback and Saving ---
        if operation_successful {
            let base_msg = format!(
                "Deleted {} row(s) from sheet '{:?}/{}'.",
                deleted_count, category, sheet_name
            );
            let final_msg = if let Some(ref err) = error_message {
                format!("{} {}", base_msg, err) // Append error if partial failure
            } else {
                base_msg
            };
            info!("{}", final_msg); // Log full message
            feedback_writer.write(SheetOperationFeedback {
                message: final_msg,
                is_error: error_message.is_some(), // Mark as error only if partial failure occurred
            });

            // Add to save list if metadata was found
            if let Some(meta) = metadata_cache {
                let key = (category.clone(), sheet_name.clone());
                sheets_to_save.insert(key, meta);
            } else if deleted_count > 0 {
                warn!(
                    "Rows deleted from '{:?}/{}' but cannot save: Metadata missing.",
                    category, sheet_name
                );
            }

            // If virtual sheet rows were deleted, propagate back to original parent cell JSON
            if is_virtual {
                if let Some(parent_ctx) = parent_ctx_opt.clone() {
                    // Reconstruct JSON from virtual sheet current grid (after deletion)
                    if let Some(vsheet) = registry.get_sheet(category, sheet_name) {
                        if vsheet.metadata.is_some() {
                            // Clone required virtual sheet info before mutable borrow of registry
                            let v_rows: Vec<Vec<String>> = vsheet.grid.clone();
                            let _ = vsheet; // release immutable borrow
                            if let Some(parent_sheet_data) = registry.get_sheet_mut(
                                &parent_ctx.parent_category,
                                &parent_ctx.parent_sheet,
                            ) {
                                if parent_ctx.parent_row < parent_sheet_data.grid.len() {
                                    if let Some(parent_row) =
                                        parent_sheet_data.grid.get_mut(parent_ctx.parent_row)
                                    {
                                        if parent_ctx.parent_col < parent_row.len() {
                                            // Build array of objects/arrays (one per virtual sheet row)
                                            let new_json = if v_rows.is_empty() {
                                                // All rows deleted => empty array
                                                "[]".to_string()
                                            } else if v_rows.len() == 1 {
                                                // Single row => store as array of strings
                                                let row_vals =
                                                    v_rows.get(0).cloned().unwrap_or_default();
                                                serde_json::Value::Array(
                                                    row_vals
                                                        .into_iter()
                                                        .map(serde_json::Value::String)
                                                        .collect(),
                                                )
                                                .to_string()
                                            } else {
                                                // Multi row => array of arrays
                                                let outer: Vec<serde_json::Value> = v_rows
                                                    .iter()
                                                    .map(|r| {
                                                        serde_json::Value::Array(
                                                            r.iter()
                                                                .cloned()
                                                                .map(serde_json::Value::String)
                                                                .collect(),
                                                        )
                                                    })
                                                    .collect();
                                                serde_json::Value::Array(outer).to_string()
                                            };
                                            if let Some(cell_ref) =
                                                parent_row.get_mut(parent_ctx.parent_col)
                                            {
                                                if *cell_ref != new_json {
                                                    *cell_ref = new_json.clone();
                                                    info!("Propagated structure row deletion to parent cell '{:?}/{}'[{},{}]", 
                                                          parent_ctx.parent_category, parent_ctx.parent_sheet, 
                                                          parent_ctx.parent_row, parent_ctx.parent_col);
                                                    if let Some(pmeta) = &parent_sheet_data.metadata
                                                    {
                                                        let key = (
                                                            parent_ctx.parent_category.clone(),
                                                            parent_ctx.parent_sheet.clone(),
                                                        );
                                                        sheets_to_save
                                                            .insert(key.clone(), pmeta.clone());
                                                        data_modified_writer.write(
                                                            SheetDataModifiedInRegistryEvent {
                                                                category: parent_ctx
                                                                    .parent_category
                                                                    .clone(),
                                                                sheet_name: parent_ctx
                                                                    .parent_sheet
                                                                    .clone(),
                                                            },
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Persist deletions to database when sheet is DB-backed
            if let Some((db_name, is_structure, row_index_values)) = db_backed {
                let base = crate::sheets::systems::io::get_default_data_base_path();
                let db_path = base.join(format!("{}.db", db_name));
                if db_path.exists() {
                    match crate::sheets::database::connection::DbConnection::open_existing(&db_path) {
                        Ok(conn) => {
                            // Foreign keys already enabled by open_existing
                            
                            // Both structure and regular tables use row_index for deletion
                            let table_type = if is_structure { "structure" } else { "regular" };
                            info!("Deleting {} {} table rows from '{}' with row_index values: {:?}", 
                                  row_index_values.len(), table_type, sheet_name, row_index_values);
                            
                            for row_index_val in row_index_values {
                                match conn.execute(
                                    &format!("DELETE FROM \"{}\" WHERE row_index = ?", sheet_name),
                                    [row_index_val],
                                ) {
                                    Ok(deleted_count) => {
                                        if deleted_count == 0 {
                                            warn!("DELETE query for row_index={} affected 0 rows in '{}'", 
                                                  row_index_val, sheet_name);
                                        } else {
                                            info!("Successfully deleted row with row_index={} from '{}' (affected {} row)", 
                                                  row_index_val, sheet_name, deleted_count);
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to delete row row_index={} from '{}': {}", 
                                               row_index_val, sheet_name, e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                "DB delete failed for '{:?}/{}': failed to open DB: {}",
                                category, sheet_name, e
                            );
                        }
                    }
                } else {
                    warn!("Database file not found: {:?}", db_path);
                }
            }
        } else if let Some(err) = error_message {
            // Only send feedback if the whole operation failed (e.g., sheet not found)
            error!(
                "Failed to delete rows from '{:?}/{}': {}",
                category, sheet_name, err
            );
            feedback_writer.write(SheetOperationFeedback {
                message: format!("Delete failed for '{:?}/{}': {}", category, sheet_name, err),
                is_error: true,
            });
        }
    } // End event loop

    // --- Trigger Saves (Immutable Borrow) ---
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref(); // Get immutable borrow for saving
        for ((cat, name), metadata) in sheets_to_save {
            info!("Rows deleted in '{:?}/{}', triggering save.", cat, name);
            // Save JSON-backed sheets (metadata.category is None for JSON files)
            // DB-backed sheets are already persisted above via direct DB operations
            if metadata.category.is_none() {
                save_single_sheet(registry_immut, &metadata); // Pass metadata
            }
        }
    }
}
