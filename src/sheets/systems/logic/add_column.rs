// src/sheets/systems/logic/add_column.rs
use crate::sheets::{
    definitions::{ColumnDataType, ColumnDefinition, SheetMetadata},
    events::{RequestAddColumn, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
// ADDED: Import HashSet (was missing in my previous response)
use std::collections::{HashMap, HashSet};

pub fn handle_add_column_request(
    mut events: EventReader<RequestAddColumn>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    let mut sheets_to_save: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category;
        let sheet_name = &event.sheet_name;

        let mut operation_successful = false;
        let mut error_message: Option<String> = None;
        let mut metadata_cache: Option<SheetMetadata> = None;
        let mut new_column_name = "New Column".to_string();

        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &mut sheet_data.metadata {
                // Determine a unique name for the new column
                let mut counter = 1;
                let existing_headers: HashSet<_> = metadata.columns.iter().map(|c| &c.header).collect();
                while existing_headers.contains(&new_column_name) {
                    new_column_name = format!("New Column {}", counter);
                    counter += 1;
                }

                let new_col_def = ColumnDefinition::new_basic(
                    new_column_name.clone(),
                    ColumnDataType::String, 
                );
                metadata.columns.push(new_col_def);

                for row in sheet_data.grid.iter_mut() {
                    row.push(String::new());
                }

                metadata.ensure_column_consistency(); 
                operation_successful = true;
                metadata_cache = Some(metadata.clone());
                data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                    category: category.clone(),
                    sheet_name: sheet_name.clone(),
                });
            } else {
                error_message = Some(format!(
                    "Metadata missing for sheet '{:?}/{}'. Cannot add column.",
                    category, sheet_name
                ));
            }
        } else {
            error_message = Some(format!(
                "Sheet '{:?}/{}' not found. Cannot add column.",
                category, sheet_name
            ));
        }

        if operation_successful {
            let msg = format!(
                "Added new column '{}' to sheet '{:?}/{}'.",
                new_column_name, category, sheet_name
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
                "Failed to add column to '{:?}/{}': {}",
                category, sheet_name, err
            );
            feedback_writer.write(SheetOperationFeedback {
                message: format!(
                    "Add column failed for '{:?}/{}': {}",
                    category, sheet_name, err
                ),
                is_error: true,
            });
        }
    }

    if !sheets_to_save.is_empty() {
        let registry_immut = registry.as_ref();
        for ((cat, name), metadata) in sheets_to_save {
            info!(
                "New column added in '{:?}/{}', triggering save.",
                cat, name
            );
            save_single_sheet(registry_immut, &metadata);
        }
    }
}