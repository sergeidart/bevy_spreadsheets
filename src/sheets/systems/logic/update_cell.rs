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
                                *cell = final_val;

                                if let Some(metadata) = &sheet_data.metadata {
                                    let key = (category.clone(), sheet_name.clone());
                                    sheets_to_save.insert(key.clone(), metadata.clone());
                                    // Mark for revalidation
                                    sheets_to_revalidate.insert(key, ());

                                    data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                                        category: category.clone(),
                                        sheet_name: sheet_name.clone(),
                                    });
                                } else {
                                    warn!("Cannot mark sheet '{:?}/{}' for save/revalidation after cell update: Metadata missing.", category, sheet_name);
                                }
                            } else {
                                trace!("Cell value unchanged for '{:?}/{}' cell[{},{}]. Skipping update.", category, sheet_name, row_idx, col_idx);
                            }
                        } else {
                            error!("Cell update failed for '{:?}/{}' cell[{},{}]: Column index invalid.", category, sheet_name, row_idx, col_idx);
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
            save_single_sheet(registry_immut, &metadata);
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
