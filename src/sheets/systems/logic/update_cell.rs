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
                        let (col_header, is_structure_col, looks_like_real_structure) =
                            if let Some(meta) = &sheet_data.metadata {
                                let header = meta
                                    .columns
                                    .get(col_idx)
                                    .map(|c| c.header.clone())
                                    .unwrap_or_default();
                                let is_struct = meta
                                    .columns
                                    .get(col_idx)
                                    .map(|c| {
                                        matches!(c.validator, Some(ColumnValidator::Structure))
                                    })
                                    .unwrap_or(false);
                                let looks_like_struct = meta.columns.len() >= 2
                                    && meta
                                        .columns
                                        .get(0)
                                        .map(|c| c.header.eq_ignore_ascii_case("id"))
                                        .unwrap_or(false)
                                    && meta
                                        .columns
                                        .get(1)
                                        .map(|c| c.header.eq_ignore_ascii_case("parent_key"))
                                        .unwrap_or(false);
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
                                    let base =
                                        crate::sheets::systems::io::get_default_data_base_path();
                                    let db_path = base.join(format!("{}.db", cat));
                                    if db_path.exists() {
                                        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                                            if looks_like_real_structure {
                                                // Skip id (0) and parent_key (1)
                                                if col_idx >= 2 {
                                                    // Safe to read id now as the mutable borrow to the cell is dropped
                                                    let id_str_opt_from_row = row.get(0).cloned();
                                                    if let (Some(id_str), Some(val)) = (
                                                        id_str_opt_from_row,
                                                        updated_val_for_db.as_ref(),
                                                    ) {
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
                                            } else {
                                                // Main-sheet structure JSON updated: reflect into structure table rows
                                                if let Some(meta) = &sheet_data.metadata {
                                                    // Determine structure schema
                                                    if let Some(col_def) = meta.columns.get(col_idx)
                                                    {
                                                        if col_def.validator
                                                            == Some(ColumnValidator::Structure)
                                                        {
                                                            if let Some(schema) =
                                                                &col_def.structure_schema
                                                            {
                                                                // Resolve parent_id of this main row
                                                                if let Ok(parent_id) = conn.query_row(
                                                                    &format!("SELECT id FROM \"{}\" WHERE row_index = ?", sheet_name),
                                                                    [row_idx as i32],
                                                                    |r| r.get::<_, i64>(0),
                                                                ) {
                                                                    let structure_table = format!("{}_{}", sheet_name, col_header);
                                                                    // Build parent_key: prefer configured key column
                                                                    let parent_key: String = if let Some(kidx) = col_def.structure_key_parent_column_index {
                                                                        row.get(kidx).cloned().unwrap_or_default()
                                                                    } else {
                                                                        // Fallback per migration
                                                                        let mut idx_opt: Option<usize> = None;
                                                                        for (i, cdef) in meta.columns.iter().enumerate() {
                                                                            if cdef.header.eq_ignore_ascii_case("Key") { idx_opt = Some(i); break; }
                                                                        }
                                                                        if idx_opt.is_none() {
                                                                            for (i, cdef) in meta.columns.iter().enumerate() {
                                                                                if cdef.header.eq_ignore_ascii_case("Name") { idx_opt = Some(i); break; }
                                                                            }
                                                                        }
                                                                        if idx_opt.is_none() {
                                                                            for (i, cdef) in meta.columns.iter().enumerate() {
                                                                                if cdef.header.eq_ignore_ascii_case("ID") { idx_opt = Some(i); break; }
                                                                            }
                                                                        }
                                                                        idx_opt.and_then(|i| row.get(i).cloned()).unwrap_or_default()
                                                                    };

                                                                    // Parse JSON from updated cell
                                                                    let parsed = updated_val_for_db.as_deref().unwrap_or("[]");
                                                                    let val: serde_json::Value = serde_json::from_str(parsed).unwrap_or(serde_json::Value::Null);

                                                                    // Helper: normalize any value to string
                                                                    fn json_to_str(v: &serde_json::Value) -> String {
                                                                        match v {
                                                                            serde_json::Value::Null => String::new(),
                                                                            serde_json::Value::Bool(b) => b.to_string(),
                                                                            serde_json::Value::Number(n) => n.to_string(),
                                                                            serde_json::Value::String(s) => s.clone(),
                                                                            _ => v.to_string(),
                                                                        }
                                                                    }

                                                                    // Expand to rows consistent with schema order
                                                                    let mut rows_to_insert: Vec<Vec<String>> = Vec::new();
                                                                    match val {
                                                                        serde_json::Value::Array(arr) => {
                                                                            if arr.iter().all(|v| v.is_object()) {
                                                                                for obj in arr {
                                                                                    if let serde_json::Value::Object(m) = obj {
                                                                                        let mut row_vec: Vec<String> = Vec::with_capacity(schema.len());
                                                                                        for f in schema { row_vec.push(m.get(&f.header).map(json_to_str).unwrap_or_default()); }
                                                                                        if row_vec.iter().any(|s| !s.trim().is_empty()) { rows_to_insert.push(row_vec); }
                                                                                    }
                                                                                }
                                                                            } else if arr.iter().all(|v| v.is_array()) {
                                                                                for a in arr {
                                                                                    if let serde_json::Value::Array(inner) = a {
                                                                                        let mut row_vec: Vec<String> = inner.iter().map(json_to_str).collect();
                                                                                        if row_vec.len() < schema.len() { row_vec.resize(schema.len(), String::new()); }
                                                                                        if row_vec.iter().any(|s| !s.trim().is_empty()) { rows_to_insert.push(row_vec); }
                                                                                    }
                                                                                }
                                                                            } else {
                                                                                // Array of primitives -> map by position
                                                                                let mut row_vec: Vec<String> = arr.iter().map(json_to_str).collect();
                                                                                if row_vec.len() < schema.len() { row_vec.resize(schema.len(), String::new()); }
                                                                                if row_vec.iter().any(|s| !s.trim().is_empty()) { rows_to_insert.push(row_vec); }
                                                                            }
                                                                        }
                                                                        serde_json::Value::Object(m) => {
                                                                            let mut row_vec: Vec<String> = Vec::with_capacity(schema.len());
                                                                            for f in schema { row_vec.push(m.get(&f.header).map(json_to_str).unwrap_or_default()); }
                                                                            if row_vec.iter().any(|s| !s.trim().is_empty()) { rows_to_insert.push(row_vec); }
                                                                        }
                                                                        _ => {}
                                                                    }

                                                                    // Replace existing child rows for this parent_id
                                                                    let tx = conn.unchecked_transaction().ok();
                                                                    if let Some(tx) = tx {
                                                                        let _ = tx.execute(
                                                                            &format!("DELETE FROM \"{}\" WHERE parent_id = ?", structure_table),
                                                                            [parent_id],
                                                                        );
                                                                        if !rows_to_insert.is_empty() {
                                                                            let field_cols = schema.iter().map(|f| format!("\"{}\"", f.header)).collect::<Vec<_>>().join(", ");
                                                                            let placeholders = std::iter::repeat("?").take(3 + schema.len()).collect::<Vec<_>>().join(", ");
                                                                            let insert_sql = format!("INSERT INTO \"{}\" (parent_id, row_index, parent_key, {}) VALUES ({})", structure_table, field_cols, placeholders);
                                                                            if let Ok(mut stmt) = tx.prepare(&insert_sql) {
                                                                                for (sidx, srow) in rows_to_insert.iter().enumerate() {
                                                                                    let mut padded = srow.clone();
                                                                                    if padded.len() < schema.len() { padded.resize(schema.len(), String::new()); }
                                                                                    let mut params: Vec<rusqlite::types::Value> = Vec::with_capacity(3 + schema.len());
                                                                                    params.push(rusqlite::types::Value::Integer(parent_id));
                                                                                    params.push(rusqlite::types::Value::Integer(sidx as i64));
                                                                                    params.push(rusqlite::types::Value::Text(parent_key.clone()));
                                                                                    for v in padded { params.push(rusqlite::types::Value::Text(v)); }
                                                                                    let _ = stmt.execute(rusqlite::params_from_iter(params.iter()));
                                                                                }
                                                                            }
                                                                        }
                                                                        let _ = tx.commit();
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
