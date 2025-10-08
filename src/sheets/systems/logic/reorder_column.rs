// src/sheets/systems/logic/reorder_column.rs
use crate::sheets::{
    definitions::SheetMetadata,
    events::{RequestReorderColumn, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use std::collections::HashMap;

pub fn handle_reorder_column_request(
    mut events: EventReader<RequestReorderColumn>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    editor_state: Option<Res<EditorWindowState>>,
) {
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let (category, sheet_name) = if let Some(state) = editor_state.as_ref() {
            if let Some(vctx) = state.virtual_structure_stack.last() {
                (&event.category, &vctx.virtual_sheet_name)
            } else {
                (&event.category, &event.sheet_name)
            }
        } else {
            (&event.category, &event.sheet_name)
        };
        let old_index = event.old_index;
        let new_index = event.new_index;

        if old_index == new_index {
            trace!(
                "Skipping reorder for sheet '{:?}/{}': old and new indices are the same ({}).",
                category,
                sheet_name,
                old_index
            );
            continue;
        }

        let mut operation_successful = false;
        let mut error_message: Option<String> = None;
        let mut metadata_cache: Option<SheetMetadata> = None;

        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &mut sheet_data.metadata {
                let num_cols = metadata.columns.len();
                if old_index < num_cols && new_index < num_cols {
                    // Reorder ColumnDefinition in metadata
                    let col_def_to_move = metadata.columns.remove(old_index);
                    metadata.columns.insert(new_index, col_def_to_move);
                    // Inline structure metadata travels with ColumnDefinition automatically.

                    // Reorder cells in each row of the grid
                    for row in sheet_data.grid.iter_mut() {
                        if old_index < row.len() {
                            // Should always be true if data is consistent
                            let cell_to_move = row.remove(old_index);
                            // new_index might need adjustment if row.len() was less than metadata.columns.len()
                            // However, assuming consistent data, direct insertion is fine.
                            row.insert(new_index, cell_to_move);
                        } else {
                            warn!("Row in sheet '{:?}/{}' has fewer cells than expected during column reorder. Row len: {}, old_index: {}. Skipping reorder for this row.", category, sheet_name, row.len(), old_index);
                        }
                    }

                    metadata.ensure_column_consistency(); // Just in case
                    operation_successful = true;
                    metadata_cache = Some(metadata.clone());
                    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                        category: category.clone(),
                        sheet_name: sheet_name.clone(),
                    });

                    // Persist column order to DB if DB-backed
                    if let Some(cat) = &metadata.category {
                        let base = crate::sheets::systems::io::get_default_data_base_path();
                        let db_path = base.join(format!("{}.db", cat));
                        if db_path.exists() {
                            if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                                // Build ordered pairs (column_name, new_index) for non-structure columns only
                                let mut pairs: Vec<(String, i32)> = Vec::new();
                                for (idx, c) in metadata.columns.iter().enumerate() {
                                    // Skip technical columns for structure sheets: id/parent_key stay 0/1 in metadata reader; our metadata here includes them only for real structure sheets
                                    pairs.push((c.header.clone(), idx as i32));
                                }
                                let _ = crate::sheets::database::writer::DbWriter::update_column_indices(
                                    &conn,
                                    sheet_name,
                                    &pairs,
                                );
                            }
                        }
                    }
                } else {
                    error_message = Some(format!(
                        "Invalid indices for reorder. Old: {}, New: {}. Total columns: {}.",
                        old_index, new_index, num_cols
                    ));
                }
            } else {
                error_message = Some(format!(
                    "Metadata missing for sheet '{:?}/{}'. Cannot reorder columns.",
                    category, sheet_name
                ));
            }
        } else {
            error_message = Some(format!(
                "Sheet '{:?}/{}' not found. Cannot reorder columns.",
                category, sheet_name
            ));
        }

        if operation_successful {
            let msg = format!(
                "Reordered column from index {} to {} in sheet '{:?}/{}'.",
                old_index, new_index, category, sheet_name
            );
            info!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: false,
            });
            if let Some(meta) = metadata_cache {
                sheets_to_save.insert((category.clone(), sheet_name.clone()), meta);
            }
        } else if let Some(err) = error_message {
            error!(
                "Failed to reorder column in '{:?}/{}': {}",
                category, sheet_name, err
            );
            feedback_writer.write(SheetOperationFeedback {
                message: format!(
                    "Column reorder failed for '{:?}/{}': {}",
                    category, sheet_name, err
                ),
                is_error: true,
            });
        }
    }

    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!("Column reordered in '{:?}/{}', triggering save.", cat, name);
            if metadata.category.is_none() {
                save_single_sheet(registry_immut, &metadata);
            }
        }
    }
}
