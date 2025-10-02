// src/sheets/systems/logic/clipboard.rs
use crate::sheets::{
    definitions::{ColumnValidator, StructureFieldDefinition},
    events::{RequestCopyCell, RequestPasteCell, SheetOperationFeedback, UpdateCellEvent},
    resources::{ClipboardBuffer, SheetRegistry},
    systems::ai::utils::parse_structure_rows_from_cell,
};
use bevy::prelude::*;

/// Handle copy cell events - copies cell value and structure data if applicable
pub fn handle_copy_cell(
    mut events: EventReader<RequestCopyCell>,
    registry: Res<SheetRegistry>,
    mut clipboard: ResMut<ClipboardBuffer>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    for event in events.read() {
        // Get the sheet
        let sheet_data = match registry.get_sheet(&event.category, &event.sheet_name) {
            Some(data) => data,
            None => {
                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "Sheet '{}/{}' not found",
                        event.category.as_deref().unwrap_or("root"),
                        event.sheet_name
                    ),
                    is_error: true,
                });
                continue;
            }
        };

        // Get the cell value
        let cell_value = sheet_data
            .grid
            .get(event.row_index)
            .and_then(|row| row.get(event.col_index))
            .cloned();

        if cell_value.is_none() {
            feedback_writer.write(SheetOperationFeedback {
                message: "Cell not found".to_string(),
                is_error: true,
            });
            continue;
        }

        let cell_value = cell_value.unwrap();

        // Get validator for this column
        let validator = sheet_data
            .metadata
            .as_ref()
            .and_then(|meta| meta.columns.get(event.col_index))
            .and_then(|col| col.validator.clone());

        // If it's a structure column, parse the structure data
        let structure_data = if matches!(validator, Some(ColumnValidator::Structure)) {
            // Get the structure schema
            if let Some(schema) = sheet_data
                .metadata
                .as_ref()
                .and_then(|meta| meta.columns.get(event.col_index))
                .and_then(|col| col.structure_schema.as_ref())
            {
                let rows = parse_structure_rows_from_cell(&cell_value, schema);
                if !rows.is_empty() {
                    Some(rows)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Update clipboard
        clipboard.cell_value = Some(cell_value.clone());
        clipboard.source_validator = validator;
        clipboard.structure_data = structure_data;

        info!(
            "Copied cell [{}/{}] at [{}, {}]",
            event.category.as_deref().unwrap_or("root"),
            event.sheet_name,
            event.row_index,
            event.col_index
        );

        feedback_writer.write(SheetOperationFeedback {
            message: "Cell copied to clipboard".to_string(),
            is_error: false,
        });
    }
}

/// Handle paste cell events - pastes cell value and structure data if applicable
pub fn handle_paste_cell(
    mut events: EventReader<RequestPasteCell>,
    registry: Res<SheetRegistry>,
    clipboard: Res<ClipboardBuffer>,
    mut cell_update_writer: EventWriter<UpdateCellEvent>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    for event in events.read() {
        // Check if clipboard has data
        if clipboard.cell_value.is_none() {
            feedback_writer.write(SheetOperationFeedback {
                message: "Clipboard is empty".to_string(),
                is_error: true,
            });
            continue;
        }

        // Get the target sheet
        let sheet_data = match registry.get_sheet(&event.category, &event.sheet_name) {
            Some(data) => data,
            None => {
                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "Sheet '{}/{}' not found",
                        event.category.as_deref().unwrap_or("root"),
                        event.sheet_name
                    ),
                    is_error: true,
                });
                continue;
            }
        };

        // Get target column validator
        let target_validator = sheet_data
            .metadata
            .as_ref()
            .and_then(|meta| meta.columns.get(event.col_index))
            .and_then(|col| col.validator.as_ref());

        // Determine what value to paste
        let paste_value = match (&clipboard.source_validator, target_validator) {
            // Both are structure columns - paste structure data
            (Some(ColumnValidator::Structure), Some(ColumnValidator::Structure)) => {
                if let Some(structure_rows) = &clipboard.structure_data {
                    // Get target schema
                    if let Some(target_schema) = sheet_data
                        .metadata
                        .as_ref()
                        .and_then(|meta| meta.columns.get(event.col_index))
                        .and_then(|col| col.structure_schema.as_ref())
                    {
                        // Convert structure rows back to JSON format
                        serialize_structure_rows(structure_rows, target_schema)
                    } else {
                        clipboard.cell_value.clone().unwrap_or_default()
                    }
                } else {
                    clipboard.cell_value.clone().unwrap_or_default()
                }
            }
            // Otherwise, paste the raw cell value
            _ => clipboard.cell_value.clone().unwrap_or_default(),
        };

        // Write update event
        cell_update_writer.write(UpdateCellEvent {
            category: event.category.clone(),
            sheet_name: event.sheet_name.clone(),
            row_index: event.row_index,
            col_index: event.col_index,
            new_value: paste_value,
        });

        info!(
            "Pasted cell to [{}/{}] at [{}, {}]",
            event.category.as_deref().unwrap_or("root"),
            event.sheet_name,
            event.row_index,
            event.col_index
        );

        feedback_writer.write(SheetOperationFeedback {
            message: "Cell pasted from clipboard".to_string(),
            is_error: false,
        });
    }
}

/// Serialize structure rows back to JSON format for storage
fn serialize_structure_rows(
    rows: &[Vec<String>],
    schema: &[StructureFieldDefinition],
) -> String {
    if rows.is_empty() {
        return String::new();
    }

    // Convert rows to JSON array of objects
    let json_array: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (idx, field) in schema.iter().enumerate() {
                let value = row.get(idx).cloned().unwrap_or_default();
                obj.insert(field.header.clone(), serde_json::Value::String(value));
            }
            serde_json::Value::Object(obj)
        })
        .collect();

    serde_json::to_string(&json_array).unwrap_or_default()
}
