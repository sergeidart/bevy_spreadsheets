// src/sheets/systems/logic/migrate_inline_structures.rs
// Migrates inline JSON data from Structure columns to separate structure sheets

use crate::sheets::{
    definitions::{
        ColumnDataType, ColumnDefinition, ColumnValidator, SheetGridData, SheetMetadata,
    },
    events::{RequestSheetRevalidation, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
};
use crate::ui::elements::editor::state::EditorWindowState;
use bevy::prelude::*;

/// Scans all sheets for Structure columns with inline JSON data and migrates them to separate structure sheets
pub fn migrate_inline_structure_data(
    registry: &mut SheetRegistry,
    feedback_writer: &mut EventWriter<SheetOperationFeedback>,
    data_modified_writer: &mut EventWriter<SheetDataModifiedInRegistryEvent>,
    revalidate_writer: &mut EventWriter<RequestSheetRevalidation>,
) -> (usize, Option<(Option<String>, String)>) {
    let mut migrated_count = 0;
    let mut structure_sheets_to_create: Vec<(
        Option<String>,
        String,
        SheetMetadata,
        Vec<Vec<String>>,
    )> = Vec::new();
    let mut cells_to_clear: Vec<(Option<String>, String, usize, usize)> = Vec::new(); // (category, sheet, row_idx, col_idx)

    // First pass: Scan for Structure columns with inline JSON
    for (category_ref, sheet_name_ref, sheet_data) in registry.iter_sheets() {
        let category = (*category_ref).clone();
        let sheet_name = sheet_name_ref.clone();

        if let Some(metadata) = &sheet_data.metadata {
            // Find Structure columns
            for (col_idx, col_def) in metadata.columns.iter().enumerate() {
                if matches!(col_def.validator, Some(ColumnValidator::Structure)) {
                    // Check if this structure has inline JSON data in any row
                    let mut has_inline_data = false;
                    let structure_sheet_name = format!("{}_{}", sheet_name, col_def.header);

                    // Collect all data to migrate
                    let mut structure_rows: Vec<(String, Vec<Vec<String>>)> = Vec::new(); // (parent_key, rows)

                    for (row_idx, row) in sheet_data.grid.iter().enumerate() {
                        if col_idx < row.len() {
                            let cell = &row[col_idx];
                            let trimmed = cell.trim();

                            // Check if cell contains JSON for migration (arrays or objects)
                            if !trimmed.is_empty() {
                                let parse_attempt = if trimmed.starts_with('[') {
                                    parse_inline_json_to_rows(trimmed, &col_def.structure_schema)
                                } else if trimmed.starts_with('{') {
                                    // Wrap single object into an array for uniform handling
                                    let wrapped = format!("[{}]", trimmed);
                                    parse_inline_json_to_rows(&wrapped, &col_def.structure_schema)
                                } else {
                                    // Non-JSON or single scalar: ignore
                                    Err("Not JSON array/object".to_string())
                                };

                                if let Ok(parsed_rows) = parse_attempt {
                                    if !parsed_rows.is_empty() {
                                        has_inline_data = true;

                                        // Generate parent_key from row index or use first column value
                                        let parent_key = if !row.is_empty() {
                                            row[0].clone() // Use first column as parent key
                                        } else {
                                            row_idx.to_string()
                                        };

                                        structure_rows.push((parent_key, parsed_rows));
                                        cells_to_clear.push((
                                            category.clone(),
                                            sheet_name.clone(),
                                            row_idx,
                                            col_idx,
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    // Create structure sheet if we found inline data
                    if has_inline_data {
                        let structure_metadata = create_structure_sheet_metadata(
                            &category,
                            &structure_sheet_name,
                            &col_def.structure_schema,
                        );

                        // Build grid with unique IDs and parent keys
                        let mut structure_grid: Vec<Vec<String>> = Vec::new();
                        for (parent_key, rows_for_parent) in structure_rows {
                            for row_data in rows_for_parent {
                                // Generate unique ID
                                let unique_id = format!(
                                    "{}-{}",
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis(),
                                    uuid::Uuid::new_v4()
                                        .to_string()
                                        .split('-')
                                        .next()
                                        .unwrap_or("0")
                                );

                                // Build full row: [id, parent_key, ...data]
                                let mut full_row = vec![unique_id, parent_key.clone()];
                                full_row.extend(row_data);
                                structure_grid.push(full_row);
                            }
                        }

                        structure_sheets_to_create.push((
                            category.clone(),
                            structure_sheet_name,
                            structure_metadata,
                            structure_grid,
                        ));

                        migrated_count += 1;
                    }
                }
            }
        }
    }

    // Second pass: Create structure sheets and clear inline JSON
    let mut first_created: Option<(Option<String>, String)> = None;
    for (category, sheet_name, metadata, grid) in structure_sheets_to_create {
        info!(
            "Creating structure sheet '{:?}/{}' from migrated inline data",
            category, sheet_name
        );

        let sheet_data = SheetGridData {
            metadata: Some(metadata.clone()),
            grid,
        };

        registry.add_or_replace_sheet(category.clone(), sheet_name.clone(), sheet_data);
        if first_created.is_none() {
            first_created = Some((category.clone(), sheet_name.clone()));
        }
        // Notify UI and render cache
        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
            category: category.clone(),
            sheet_name: sheet_name.clone(),
        });
        revalidate_writer.write(RequestSheetRevalidation {
            category: category.clone(),
            sheet_name: sheet_name.clone(),
        });

        // Do not save to JSON; DB-backed runtime will persist via DB systems.
    }

    // Clear inline JSON from parent sheets
    let mut parent_sheets_to_save: Vec<(Option<String>, String)> = Vec::new();
    for (category, sheet_name, row_idx, col_idx) in cells_to_clear {
        if let Some(sheet_data) = registry.get_sheet_mut(&category, &sheet_name) {
            if let Some(row) = sheet_data.grid.get_mut(row_idx) {
                if col_idx < row.len() {
                    row[col_idx] = String::new(); // Clear the cell
                }
            }

            // Queue for save after releasing mutable borrow
            parent_sheets_to_save.push((category.clone(), sheet_name.clone()));
            // Notify data change for parent sheet as well
            data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                category: category.clone(),
                sheet_name: sheet_name.clone(),
            });
            revalidate_writer.write(RequestSheetRevalidation {
                category: category.clone(),
                sheet_name: sheet_name.clone(),
            });
        }
    }

    // Do not save modified parent sheets to JSON in DB mode.
    for _ in parent_sheets_to_save { /* no-op */ }

    if migrated_count > 0 {
        feedback_writer.write(SheetOperationFeedback {
            message: format!(
                "Migrated {} structure column(s) from inline JSON to separate sheets.",
                migrated_count
            ),
            is_error: false,
        });
    }

    (migrated_count, first_created)
}

/// Bevy system: run inline-structure migration exactly once after sheets are loaded.
pub fn run_inline_structure_migration_once(
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
    mut revalidate_writer: EventWriter<RequestSheetRevalidation>,
    mut editor_state: Option<ResMut<EditorWindowState>>,
    mut ran: Local<bool>,
) {
    if *ran {
        return;
    }
    let (migrated, first_created) = migrate_inline_structure_data(
        &mut registry,
        &mut feedback_writer,
        &mut data_modified_writer,
        &mut revalidate_writer,
    );
    if migrated > 0 {
        info!(
            "Inline structure migration created/filled {} structure sheet(s)",
            migrated
        );
        // Auto-open the first created structure sheet (always, to let user inspect results)
        if let (Some(state), Some((cat, name))) = (editor_state.as_deref_mut(), first_created) {
            state.selected_category = cat.clone();
            state.selected_sheet_name = Some(name.clone());
            state.reset_interaction_modes_and_selections();
            // Force filter recalc so the table shows immediately
            state.force_filter_recalculation = true;
            info!(
                "Auto-selected migrated structure sheet '{:?}/{}'",
                cat, name
            );
        }
    } else {
        info!("Inline structure migration: no inline JSON found to migrate");
    }
    *ran = true;
}

/// Parse inline JSON (array of objects or array of arrays) to rows
fn parse_inline_json_to_rows(
    json_str: &str,
    schema: &Option<Vec<crate::sheets::definitions::StructureFieldDefinition>>,
) -> Result<Vec<Vec<String>>, String> {
    let val: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("JSON parse error: {}", e))?;

    if let serde_json::Value::Array(arr) = val {
        let mut rows = Vec::new();
        // Case: array of primitives -> treat as a single row
        if arr.iter().all(|v| {
            matches!(
                v,
                serde_json::Value::String(_)
                    | serde_json::Value::Number(_)
                    | serde_json::Value::Bool(_)
                    | serde_json::Value::Null
            )
        }) {
            let row: Vec<String> = arr
                .iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Null => String::new(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                })
                .collect();
            rows.push(row);
            return Ok(rows);
        }

        for item in arr {
            match item {
                // Array of arrays: [["val1", "val2"], ["val3", "val4"]]
                serde_json::Value::Array(row_arr) => {
                    let row: Vec<String> = row_arr
                        .iter()
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            serde_json::Value::Null => String::new(),
                            _ => serde_json::to_string(v).unwrap_or_default(),
                        })
                        .collect();
                    rows.push(row);
                }
                // Array of objects: [{"field1": "val1", "field2": "val2"}]
                serde_json::Value::Object(obj) => {
                    let row = if let Some(schema_fields) = schema {
                        // Use schema to determine field order
                        schema_fields
                            .iter()
                            .map(|field| {
                                obj.get(&field.header)
                                    .map(|v| match v {
                                        serde_json::Value::String(s) => s.clone(),
                                        serde_json::Value::Number(n) => n.to_string(),
                                        serde_json::Value::Bool(b) => b.to_string(),
                                        serde_json::Value::Null => String::new(),
                                        _ => serde_json::to_string(v).unwrap_or_default(),
                                    })
                                    .unwrap_or_default()
                            })
                            .collect()
                    } else {
                        // No schema, just collect values in arbitrary order
                        obj.values()
                            .map(|v| match v {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::Bool(b) => b.to_string(),
                                serde_json::Value::Null => String::new(),
                                _ => serde_json::to_string(v).unwrap_or_default(),
                            })
                            .collect()
                    };
                    rows.push(row);
                }
                _ => {} // Skip other types
            }
        }

        Ok(rows)
    } else {
        Err("Expected JSON array".to_string())
    }
}

/// Create metadata for a structure sheet
fn create_structure_sheet_metadata(
    category: &Option<String>,
    sheet_name: &str,
    schema: &Option<Vec<crate::sheets::definitions::StructureFieldDefinition>>,
) -> SheetMetadata {
    let mut columns = vec![
        ColumnDefinition {
            header: "id".to_string(),
            validator: None,
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            deleted: false,
            hidden: false, // Legacy, will be filtered by reader/writer
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        },
        ColumnDefinition {
            header: "parent_key".to_string(),
            validator: None,
            data_type: ColumnDataType::String,
            filter: None,
            ai_context: None,
            ai_enable_row_generation: None,
            ai_include_in_send: None,
            deleted: false,
            hidden: false, // Legacy, will be filtered by reader/writer
            width: None,
            structure_schema: None,
            structure_column_order: None,
            structure_key_parent_column_index: None,
            structure_ancestor_key_parent_column_indices: None,
        },
    ];

    // Add schema columns
    if let Some(schema_fields) = schema {
        for field in schema_fields {
            columns.push(ColumnDefinition {
                header: field.header.clone(),
                validator: None,
                data_type: field.data_type.clone(),
                filter: None,
                ai_context: None,
                ai_enable_row_generation: None,
                ai_include_in_send: None,
                deleted: false,
                hidden: false, // User-defined schema field
                width: None,
                structure_schema: None,
                structure_column_order: None,
                structure_key_parent_column_index: None,
                structure_ancestor_key_parent_column_indices: None,
            });
        }
    }

    let data_filename = format!("{}.json", sheet_name);
    let mut meta = SheetMetadata::create_generic(
        sheet_name.to_string(),
        data_filename,
        columns.len(),
        category.clone(),
    );
    meta.columns = columns;
    // Structure tables default to hidden
    meta.hidden = true;
    meta
}
