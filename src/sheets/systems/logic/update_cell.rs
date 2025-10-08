// src/sheets/systems/logic/update_cell.rs
use crate::sheets::{
    definitions::{ColumnValidator, SheetMetadata},
    // ADDED RequestSheetRevalidation
    events::{
        RequestSheetRevalidation, SheetDataModifiedInRegistryEvent, SheetOperationFeedback,
        UpdateCellEvent,
    },
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;
use std::collections::HashMap;

pub fn handle_cell_update(
    mut events: EventReader<UpdateCellEvent>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    // ADDED revalidation writer
    mut revalidate_writer: EventWriter<RequestSheetRevalidation>,
    editor_state: Option<Res<EditorWindowState>>, // To map virtual sheets to parent contexts
) {
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();
    // ADDED: Track sheets needing revalidation
    let mut sheets_to_revalidate: HashMap<(Option<String>, String), ()> = HashMap::new();

    for event in events.read() {
        let category = event.category.clone();
        let sheet_name = event.sheet_name.clone();
        let mut is_virtual = false;
        let mut parent_ctx_opt = None;
        if let Some(state) = editor_state.as_ref() {
            if sheet_name.starts_with("__virtual__") {
                // Find corresponding context
                if let Some(vctx) = state
                    .virtual_structure_stack
                    .iter()
                    .find(|v| v.virtual_sheet_name == sheet_name)
                {
                    is_virtual = true;
                    parent_ctx_opt = Some(vctx.parent.clone());
                }
            }
        }
        let row_idx = event.row_index;
        let col_idx = event.col_index;
        let new_value = &event.new_value;

        let validation_result: Result<(), String> = {
            let registry_immut = registry.as_ref();
            if let Some(sheet_data) = registry_immut.get_sheet(&category, &sheet_name) {
                if let Some(row) = sheet_data.grid.get(row_idx) {
                    if row.get(col_idx).is_some() {
                        Ok(())
                    } else {
                        Err(format!(
                            "Column index {} out of bounds ({} columns).",
                            col_idx,
                            row.len()
                        ))
                    }
                } else {
                    Err(format!(
                        "Row index {} out of bounds ({} rows).",
                        row_idx,
                        sheet_data.grid.len()
                    ))
                }
            } else {
                Err(format!("Sheet '{:?}/{}' not found.", category, sheet_name))
            }
        };

        match validation_result {
            Ok(()) => {
                if let Some(sheet_data) = registry.get_sheet_mut(&category, &sheet_name) {
                    if let Some(row) = sheet_data.grid.get_mut(row_idx) {
                        // Precompute column metadata details outside of the mutable cell borrow scope
                        let (col_header, is_structure_col, looks_like_real_structure) = if let Some(meta) = &sheet_data.metadata {
                            let header = meta
                                .columns
                                .get(col_idx)
                                .map(|c| c.header.clone())
                                .unwrap_or_default();
                            let is_struct = meta
                                .columns
                                .get(col_idx)
                                .map(|c| matches!(c.validator, Some(ColumnValidator::Structure)))
                                .unwrap_or(false);
                            let looks_like_struct = meta.columns.len() >= 2
                                && meta.columns.get(0).map(|c| c.header.eq_ignore_ascii_case("id")).unwrap_or(false)
                                && meta.columns.get(1).map(|c| c.header.eq_ignore_ascii_case("parent_key")).unwrap_or(false);
                            (header, is_struct, looks_like_struct)
                        } else {
                            (String::new(), false, false)
                        };

                        let mut changed = false;
                        let mut updated_val_for_db: Option<String> = None;

                        // Narrow scope of the mutable borrow to avoid conflicts when later reading row id
                        {
                            if let Some(cell) = row.get_mut(col_idx) {
                                if *cell != *new_value {
                                    let mut final_val = new_value.clone();
                                    // Normalize if structure column: wrap single object into array, ensure array of objects, remove legacy linkage keys
                                    if let Some(meta) = &sheet_data.metadata {
                                        if let Some(col_def) = meta.columns.get(col_idx) {
                                            if matches!(
                                                col_def.validator,
                                                Some(ColumnValidator::Structure)
                                            ) {
                                                if let Ok(parsed) =
                                                    serde_json::from_str::<serde_json::Value>(
                                                        &final_val,
                                                    )
                                                {
                                                    use serde_json::{Map, Value};
                                                    let mut arr: Vec<Value> = match parsed {
                                                        Value::Object(o) => vec![Value::Object(o)],
                                                        Value::Array(a) => a,
                                                        other => {
                                                            let mut m = Map::new();
                                                            m.insert("value".into(), other);
                                                            vec![Value::Object(m)]
                                                        }
                                                    };
                                                    for v in arr.iter_mut() {
                                                        if let Value::Object(o) = v {
                                                            o.remove("source_column_indices");
                                                        }
                                                    }
                                                    final_val = Value::Array(arr).to_string();
                                                } else {
                                                    // If invalid JSON, store empty array
                                                    final_val = "[]".to_string();
                                                }
                                            }
                                        }
                                    }
                                    trace!(
                                        "Updating cell [{},{}] in sheet '{:?}/{}' from '{}' to '{}'",
                                        row_idx,
                                        col_idx,
                                        category,
                                        sheet_name,
                                        cell,
                                        final_val
                                    );
                                    *cell = final_val.clone();
                                    updated_val_for_db = Some(final_val);
                                    changed = true;
                                } else {
                                    trace!("Cell value unchanged for '{:?}/{}' cell[{},{}]. Skipping update.", category, sheet_name, row_idx, col_idx);
                                }
                            } else {
                                error!("Cell update failed for '{:?}/{}' cell[{},{}]: Column index invalid.", category, sheet_name, row_idx, col_idx);
                            }
                        }

                        if changed {
                            if let Some(metadata) = &sheet_data.metadata {
                                let key = (category.clone(), sheet_name.clone());
                                sheets_to_save.insert(key.clone(), metadata.clone());
                                // Mark for revalidation
                                sheets_to_revalidate.insert(key, ());

                                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                                    category: category.clone(),
                                    sheet_name: sheet_name.clone(),
                                });
                                // Persist to DB for DB-backed sheets
                                if let Some(cat) = &metadata.category {
                                    let base = crate::sheets::systems::io::get_default_data_base_path();
                                    let db_path = base.join(format!("{}.db", cat));
                                    if db_path.exists() {
                                        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                                            if looks_like_real_structure {
                                                // Skip id (0) and parent_key (1)
                                                if col_idx >= 2 {
                                                    // Safe to read id now as the mutable borrow to the cell is dropped
                                                    let id_str_opt_from_row = row.get(0).cloned();
                                                    if let (Some(id_str), Some(val)) = (id_str_opt_from_row, updated_val_for_db.as_ref()) {
                                                        if let Ok(row_id) = id_str.parse::<i64>() {
                                                            let _ = crate::sheets::database::writer::DbWriter::update_structure_cell_by_id(
                                                                &conn,
                                                                &sheet_name,
                                                                row_id,
                                                                &col_header,
                                                                val,
                                                            );
                                                        }
                                                    }
                                                }
                                            } else if !is_structure_col {
                                                if let Some(val) = updated_val_for_db.as_ref() {
                                                    let _ = crate::sheets::database::writer::DbWriter::update_cell(
                                                        &conn,
                                                        &sheet_name,
                                                        row_idx,
                                                        &col_header,
                                                        val,
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                warn!("Cannot mark sheet '{:?}/{}' for save/revalidation after cell update: Metadata missing.", category, sheet_name);
                            }
                        }
                    } else {
                        error!(
                            "Cell update failed for '{:?}/{}' cell[{},{}]: Row index invalid.",
                            category, sheet_name, row_idx, col_idx
                        );
                    }
                } else {
                    error!(
                        "Cell update failed for '{:?}/{}' cell[{},{}]: Sheet invalid.",
                        category, sheet_name, row_idx, col_idx
                    );
                }
            }
            Err(err_msg) => {
                let full_msg = format!(
                    "Cell update rejected for sheet '{:?}/{}' cell[{},{}]: {}",
                    category, sheet_name, row_idx, col_idx, err_msg
                );
                warn!("{}", full_msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: full_msg,
                    is_error: true,
                });
            }
        }

        // If virtual sheet cell changed, propagate back to original parent cell JSON
        if is_virtual {
            if let Some(parent_ctx) = parent_ctx_opt.clone() {
                // Reconstruct JSON from virtual sheet current grid
                if let Some(vsheet) = registry.get_sheet(&category, &sheet_name) {
                    if vsheet.metadata.is_some() {
                        // Clone required virtual sheet info before mutable borrow of registry
                        let v_rows: Vec<Vec<String>> = vsheet.grid.clone();
                        let _ = vsheet; // release immutable borrow
                        if let Some(parent_sheet_data) = registry
                            .get_sheet_mut(&parent_ctx.parent_category, &parent_ctx.parent_sheet)
                        {
                            if parent_ctx.parent_row < parent_sheet_data.grid.len() {
                                if let Some(parent_row) =
                                    parent_sheet_data.grid.get_mut(parent_ctx.parent_row)
                                {
                                    if parent_ctx.parent_col < parent_row.len() {
                                        // Build array of objects (one per virtual sheet row)
                                        let new_json = if v_rows.len() <= 1 {
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
                                                if let Some(pmeta) = &parent_sheet_data.metadata {
                                                    let key = (
                                                        parent_ctx.parent_category.clone(),
                                                        parent_ctx.parent_sheet.clone(),
                                                    );
                                                    sheets_to_save
                                                        .insert(key.clone(), pmeta.clone());
                                                    sheets_to_revalidate.insert(key.clone(), ());
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
    }

    // Save sheets that were modified
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!(
                "Cell updated in '{:?}/{}', triggering immediate save.",
                cat, name
            );
            if metadata.category.is_none() {
                save_single_sheet(registry_immut, &metadata);
            }
        }
    }

    // Send revalidation requests
    for (cat, name) in sheets_to_revalidate.keys() {
        revalidate_writer.write(RequestSheetRevalidation {
            category: cat.clone(),
            sheet_name: name.clone(),
        });
        trace!("Sent revalidation request for sheet '{:?}/{}'.", cat, name);
    }
}
