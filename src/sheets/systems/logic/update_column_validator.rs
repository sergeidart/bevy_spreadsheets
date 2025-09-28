// src/sheets/systems/logic/update_column_validator.rs (recreated cleanly)
use crate::sheets::{
    definitions::{ColumnDataType, ColumnValidator, SheetMetadata, StructureFieldDefinition},
    events::{
        RequestSheetRevalidation, RequestUpdateColumnValidator, SheetDataModifiedInRegistryEvent,
        SheetOperationFeedback,
    },
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use serde_json::Value;
use std::collections::HashMap;

/// Handles requests to update the validator (and derived base data type) for a specific column.
/// Supports the new Structure validator which snapshots selected source columns into a JSON object
/// stored directly in the target column cells as a string.
pub fn handle_update_column_validator(
    mut events: EventReader<RequestUpdateColumnValidator>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut revalidation_writer: EventWriter<RequestSheetRevalidation>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    // Track sheets whose metadata changed so we can save after loop with immutable borrow
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category;
        let sheet_name = &event.sheet_name;
        let col_index = event.column_index;
        let new_validator_opt = &event.new_validator;

        // --- Phase 1: Validation (immutable) ---
        let validation_result: Result<(), String> = (|| {
            let sheet_data = registry
                .get_sheet(category, sheet_name)
                .ok_or_else(|| format!("Sheet '{:?}/{}' not found.", category, sheet_name))?;
            let metadata = sheet_data
                .metadata
                .as_ref()
                .ok_or_else(|| "Metadata missing.".to_string())?;
            if col_index >= metadata.columns.len() {
                return Err(format!(
                    "Column index {} out of bounds ({} columns).",
                    col_index,
                    metadata.columns.len()
                ));
            }
            if let Some(v) = new_validator_opt {
                match v {
                    ColumnValidator::Basic(_) => {}
                    ColumnValidator::Linked {
                        target_sheet_name,
                        target_column_index,
                    } => {
                        // Look for target sheet anywhere (category-agnostic) for convenience
                        let mut found_sheet_meta = None;
                        for (_cat, name, data) in registry.iter_sheets() {
                            if name == target_sheet_name {
                                found_sheet_meta = data.metadata.as_ref();
                                break;
                            }
                        }
                        let target_meta = found_sheet_meta.ok_or_else(|| {
                            format!("Target sheet '{}' not found.", target_sheet_name)
                        })?;
                        if *target_column_index >= target_meta.columns.len() {
                            return Err(format!(
                                "Target column index {} out of bounds for sheet '{}' ({} columns).",
                                target_column_index,
                                target_sheet_name,
                                target_meta.columns.len()
                            ));
                        }
                        // Prevent linking to itself (same category, same sheet, same column)
                        if target_sheet_name == sheet_name
                            && *target_column_index == col_index
                            && target_meta.category == *category
                        {
                            return Err("Cannot link column to itself.".to_string());
                        }
                    }
                    ColumnValidator::Structure => { /* Schema validated separately when schema provided */
                    }
                }
            }
            Ok(())
        })();

        if let Err(err_msg) = validation_result {
            let msg = format!(
                "Failed validator update for col {} of sheet '{:?}/{}': {}",
                col_index + 1,
                category,
                sheet_name,
                err_msg
            );
            error!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
            continue;
        }

        // --- Phase 2: Apply (mutable) ---
        // (Structure schema handled elsewhere; no indices-based sources needed)
        if let Some(sheet_data_mut) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(meta_mut) = &mut sheet_data_mut.metadata {
                if col_index >= meta_mut.columns.len() {
                    // Should not happen after validation
                    error!("Column index out of bounds during apply phase.");
                    continue;
                }

                // Snapshot old column definition & cell values before mutating (needed if self is included as source)
                let old_col_def_snapshot = meta_mut.columns[col_index].clone();
                let old_validator = old_col_def_snapshot.validator.clone();
                let old_was_structure = matches!(old_validator, Some(ColumnValidator::Structure));
                let old_self_cells: Vec<String> = sheet_data_mut
                    .grid
                    .iter()
                    .map(|row| row.get(col_index).cloned().unwrap_or_default())
                    .collect();
                meta_mut.columns[col_index].validator = new_validator_opt.clone();
                // Derive data type
                let derived_type = match &meta_mut.columns[col_index].validator {
                    Some(ColumnValidator::Basic(t)) => *t,
                    Some(ColumnValidator::Linked { .. }) => ColumnDataType::String,
                    Some(ColumnValidator::Structure) => ColumnDataType::String,
                    None => ColumnDataType::String,
                };
                meta_mut.columns[col_index].data_type = derived_type;
                // New Structure variant: structure schema is stored in col_def.structure_schema (not handled here yet)

                // Feedback message (primary)
                let new_validator_ref = &meta_mut.columns[col_index].validator;
                let change_msg = match (&old_validator, new_validator_ref) {
                    (Some(o), Some(n)) => format!("Changed validator {:?} -> {:?}.", o, n),
                    (Some(o), None) => format!("Cleared validator {:?}.", o),
                    (None, Some(n)) => format!("Set validator {:?}.", n),
                    (None, None) => "Validator unchanged.".to_string(),
                };
                let base_msg = format!(
                    "Updated validator for column {} ('{}') in sheet '{:?}/{}': {} Base type now {:?}.",
                    col_index + 1,
                    meta_mut.columns[col_index].header.clone(),
                    category,
                    sheet_name,
                    change_msg,
                    meta_mut.columns[col_index].data_type
                );
                info!("{}", base_msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: base_msg,
                    is_error: false,
                });

                // Row population (if any) happens below; we'll clone metadata for saving after modifications.

                // If new validator is Structure: populate each row cell with JSON object of selected source columns
                if matches!(
                    meta_mut.columns[col_index].validator,
                    Some(ColumnValidator::Structure)
                ) {
                    if meta_mut.columns[col_index].structure_schema.is_none() {
                        if let Some(sources) = &event.structure_source_columns {
                            // Pre-collect source column definitions to avoid borrowing meta_mut during mutation loop
                            let mut seen = std::collections::HashSet::new();
                            let mut collected_defs: Vec<StructureFieldDefinition> = Vec::new();
                            // (src_index, is_self)
                            let mut value_sources: Vec<(usize, bool)> = Vec::new();
                            let columns_snapshot = meta_mut.columns.clone();
                            for src in sources.iter().copied() {
                                if seen.insert(src) {
                                    if src == col_index {
                                        // Use old column definition (pre-conversion) so inner field reflects old Basic/Linked type
                                        let mut def =
                                            StructureFieldDefinition::from(&old_col_def_snapshot);
                                        // If UI supplied explicit original validator, prefer it (old_col_def_snapshot might already be Structure if premature mutation elsewhere)
                                        if let Some(orig) = event.original_self_validator.clone() {
                                            def.validator = Some(orig.clone());
                                            def.data_type = match orig {
                                                ColumnValidator::Basic(t) => t,
                                                ColumnValidator::Linked { .. } => {
                                                    ColumnDataType::String
                                                }
                                                ColumnValidator::Structure => {
                                                    ColumnDataType::String
                                                }
                                            };
                                        }
                                        // Never allow nested structure-of-structure for the self snapshot: if validator still Structure downgrade to String basic
                                        if matches!(def.validator, Some(ColumnValidator::Structure))
                                        {
                                            def.validator = Some(ColumnValidator::Basic(
                                                ColumnDataType::String,
                                            ));
                                            def.data_type = ColumnDataType::String;
                                        }
                                        collected_defs.push(def);
                                        value_sources.push((src, true));
                                    } else if let Some(src_col) = columns_snapshot.get(src) {
                                        collected_defs
                                            .push(StructureFieldDefinition::from(src_col));
                                        value_sources.push((src, false));
                                    }
                                }
                            }
                            meta_mut.columns[col_index].structure_schema =
                                Some(collected_defs.clone());
                            meta_mut.columns[col_index].structure_column_order =
                                Some((0..collected_defs.len()).collect());
                            if meta_mut.columns[col_index]
                                .structure_key_parent_column_index
                                .is_none()
                            {
                                if let Some(k) = event.key_parent_column_index {
                                    meta_mut.columns[col_index].structure_key_parent_column_index =
                                        Some(k);
                                }
                            }
                            for (row_idx, row) in sheet_data_mut.grid.iter_mut().enumerate() {
                                if row.len() <= col_index {
                                    row.resize(col_index + 1, String::new());
                                }
                                if value_sources.is_empty() {
                                    row[col_index] = "[]".to_string();
                                } else {
                                    let mut vec_vals: Vec<Value> =
                                        Vec::with_capacity(value_sources.len());
                                    for (src_idx, is_self) in value_sources.iter() {
                                        if *is_self {
                                            // Use pre-conversion cell value
                                            let val = old_self_cells
                                                .get(row_idx)
                                                .cloned()
                                                .unwrap_or_default();
                                            vec_vals.push(Value::String(val));
                                        } else {
                                            let val =
                                                row.get(*src_idx).cloned().unwrap_or_default();
                                            vec_vals.push(Value::String(val));
                                        }
                                    }
                                    row[col_index] = Value::Array(vec_vals).to_string();
                                }
                            }
                        } else {
                            for row in sheet_data_mut.grid.iter_mut() {
                                if row.len() <= col_index {
                                    row.resize(col_index + 1, String::new());
                                }
                                if let Some(cell) = row.get_mut(col_index) {
                                    if cell.trim().is_empty() {
                                        *cell = "[]".to_string();
                                    }
                                }
                            }
                        }
                    } else {
                        // Ensure existing cells not empty
                        for row in sheet_data_mut.grid.iter_mut() {
                            if row.len() <= col_index {
                                row.resize(col_index + 1, String::new());
                            }
                            if let Some(cell) = row.get_mut(col_index) {
                                if cell.trim().is_empty() {
                                    *cell = "[]".to_string();
                                }
                            }
                        }
                    }
                } else if old_was_structure
                    && !matches!(
                        meta_mut.columns[col_index].validator,
                        Some(ColumnValidator::Structure)
                    )
                {
                    // Conversion AWAY from Structure: flatten existing JSON object content into semi-readable single-line string.
                    // We keep data but warn about potential loss of structured editing.
                    let warn_msg = format!(
                        "Warning: Converted Structure column '{}' to new validator. JSON objects flattened into 'key=value; key2=value2' strings (data may no longer be editable as structured).",
                        meta_mut.columns[col_index].header
                    );
                    warn!("{}", warn_msg);
                    feedback_writer.write(SheetOperationFeedback {
                        message: warn_msg,
                        is_error: false,
                    });
                    for row in sheet_data_mut.grid.iter_mut() {
                        if row.len() <= col_index {
                            continue;
                        }
                        if let Some(cell) = row.get_mut(col_index) {
                            let trimmed = cell.trim();
                            if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                                match val {
                                    Value::Object(map) => {
                                        let mut parts: Vec<String> = map
                                            .iter()
                                            .map(|(k, v)| {
                                                format!(
                                                    "{}={}",
                                                    k,
                                                    v.as_str().unwrap_or(&v.to_string())
                                                )
                                            })
                                            .collect();
                                        parts.sort();
                                        *cell = parts.join("; ");
                                    }
                                    Value::Array(arr) => {
                                        // Array of strings => join; Array of arrays => join rows with |; Array of objects => legacy -> key=value pairs per obj
                                        if arr.iter().all(|v| v.is_string()) {
                                            let parts: Vec<String> = arr
                                                .iter()
                                                .map(|v| v.as_str().unwrap_or("").to_string())
                                                .collect();
                                            *cell = parts.join("; ");
                                        } else if arr.iter().all(|v| v.is_array()) {
                                            let row_strings: Vec<String> = arr
                                                .iter()
                                                .map(|row| {
                                                    if let Value::Array(inner) = row {
                                                        inner
                                                            .iter()
                                                            .map(|v| v.as_str().unwrap_or(""))
                                                            .collect::<Vec<_>>()
                                                            .join(";")
                                                    } else {
                                                        String::new()
                                                    }
                                                })
                                                .collect();
                                            *cell = row_strings.join(" | ");
                                        } else if arr.iter().all(|v| v.is_object()) {
                                            let mut rows: Vec<String> = Vec::new();
                                            for obj in arr {
                                                if let Value::Object(m) = obj {
                                                    let mut parts: Vec<String> = m
                                                        .iter()
                                                        .map(|(k, v)| {
                                                            format!(
                                                                "{}={}",
                                                                k,
                                                                v.as_str()
                                                                    .unwrap_or(&v.to_string())
                                                            )
                                                        })
                                                        .collect();
                                                    parts.sort();
                                                    rows.push(parts.join(";"));
                                                }
                                            }
                                            *cell = rows.join(" | ");
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                // After any potential row mutations, record metadata clone for save
                // Emit data modified event so downstream systems (structure sync) run.
                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });
                sheets_to_save.insert((category.clone(), sheet_name.clone()), meta_mut.clone());
            } else {
                error!(
                    "Metadata missing during apply phase for sheet '{:?}/{}'.",
                    category, sheet_name
                );
                continue;
            }
        } else {
            error!(
                "Sheet '{:?}/{}' disappeared before apply phase.",
                category, sheet_name
            );
            continue;
        }

        // Request revalidation (render cache rebuild etc.)
        revalidation_writer.write(RequestSheetRevalidation {
            category: category.clone(),
            sheet_name: sheet_name.clone(),
        });
    }

    // --- Phase 3: Saving (immutable borrow) ---
    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!(
                "Validator updated for '{:?}/{}', triggering save.",
                cat, name
            );
            save_single_sheet(registry_immut, &metadata);
        }
    }
}
