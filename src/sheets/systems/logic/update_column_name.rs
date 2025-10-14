// src/sheets/systems/logic/update_column_name.rs
use crate::sheets::{
    definitions::SheetMetadata, // Need metadata for saving
    events::{RequestUpdateColumnName, SheetDataModifiedInRegistryEvent, SheetOperationFeedback},
    resources::SheetRegistry,
    systems::io::save::save_single_sheet,
};
use bevy::prelude::*;
use std::collections::HashMap; // Keep HashMap

/// Handles requests to update the name of a specific column in a sheet's metadata.
pub fn handle_update_column_name(
    mut events: EventReader<RequestUpdateColumnName>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>,
) {
    // Use map to track sheets needing save
    let mut changed_sheets: HashMap<(Option<String>, String), SheetMetadata> = HashMap::new();

    for event in events.read() {
        let category = &event.category; // Get category
        let sheet_name = &event.sheet_name;
        let col_index = event.column_index;
        let new_name = event.new_name.trim(); // Trim whitespace

        let mut success = false; // Track if update was successful for this event
        let mut metadata_cache: Option<SheetMetadata> = None; // Cache metadata for saving

        // --- Validation ---
        if new_name.is_empty() {
            feedback_writer.write(SheetOperationFeedback {
                message: format!(
                    "Failed column rename in '{:?}/{}': New name cannot be empty.",
                    category, sheet_name
                ),
                is_error: true,
            });
            continue; // Skip to next event
        }
        // Basic filename character check (optional, but good practice)
        if new_name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
            feedback_writer.write(SheetOperationFeedback {
                message: format!(
                    "Failed column rename in '{:?}/{}': New name '{}' contains invalid characters.",
                    category, sheet_name, new_name
                ),
                is_error: true,
            });
            continue;
        }

        // --- Apply Update (Mutable Borrow) ---
        // Get sheet using category
        // Precompute context flags without holding immutable borrows during mutation
        let (was_structure_before, is_real_structure_sheet_now) = {
            let was_structure = registry
                .get_sheet(category, sheet_name)
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.columns.get(col_index))
                .map(|c| {
                    matches!(
                        c.validator,
                        Some(crate::sheets::definitions::ColumnValidator::Structure)
                    )
                })
                .unwrap_or(false);
            let is_structure_sheet = registry.is_structure_table(category, sheet_name);
            (was_structure, is_structure_sheet)
        };

        if let Some(sheet_data) = registry.get_sheet_mut(category, sheet_name) {
            if let Some(metadata) = &mut sheet_data.metadata {
                // --- CORRECTED: Check bounds using columns.len() ---
                if col_index < metadata.columns.len() {
                    // --- CORRECTED: Check duplicates using columns[idx].header ---
                    let is_duplicate = metadata
                        .columns // Iterate over columns
                        .iter()
                        .enumerate()
                        .any(|(idx, c)| {
                            // c is &ColumnDefinition
                            // Exclude deleted columns from duplicate check
                            idx != col_index && !c.deleted && c.header.eq_ignore_ascii_case(new_name)
                            // Access header field
                        });
                    
                    debug!("Column rename '{}' -> '{}': duplicate check = {}", 
                        metadata.columns.get(col_index).map(|c| c.header.as_str()).unwrap_or("?"),
                        new_name, is_duplicate);
                    
                    if is_duplicate {
                        feedback_writer.write(SheetOperationFeedback {
                            message: format!(
                                "Failed column rename in '{:?}/{}': Name '{}' already exists.",
                                category, sheet_name, new_name
                            ),
                            is_error: true,
                        });
                        // continue; // Decide if duplicate check should stop processing
                    } else {
                        // --- CORRECTED: Perform rename on columns[idx].header ---
                        let old_name = std::mem::replace(
                            &mut metadata.columns[col_index].header, // Access header field
                            new_name.to_string(),
                        );
                        let msg = format!(
                            "Renamed column {} in sheet '{:?}/{}' from '{}' to '{}'.",
                            col_index + 1, // User-facing index
                            category,
                            sheet_name,
                            old_name,
                            new_name
                        );
                        info!("{}", msg);
                        feedback_writer.write(SheetOperationFeedback {
                            message: msg,
                            is_error: false,
                        });
                        // Persist rename to DB when DB-backed
                        if let Some(cat) = &metadata.category {
                            debug!("Persisting column rename to DB: category={}, sheet={}, old={}, new={}", 
                                cat, sheet_name, old_name, new_name);
                            let base = crate::sheets::systems::io::get_default_data_base_path();
                            let db_path = base.join(format!("{}.db", cat));
                            if db_path.exists() {
                                match rusqlite::Connection::open(&db_path) {
                                    Ok(conn) => {
                                        if is_real_structure_sheet_now {
                                            // Rename actual column in structure table data + metadata
                                            debug!("Renaming data column in structure table");
                                            match crate::sheets::database::writer::DbWriter::rename_data_column(
                                                &conn,
                                                sheet_name,
                                                &old_name,
                                                new_name,
                                            ) {
                                                Ok(_) => info!("Successfully renamed column in DB"),
                                                Err(e) => error!("Failed to rename column in DB: {}", e),
                                            }
                                        } else {
                                            // Parent main table column rename
                                            // If it's a Structure validator column being renamed, rename the underlying structure table as well
                                            if was_structure_before {
                                                debug!("Renaming structure table");
                                                match crate::sheets::database::writer::DbWriter::rename_structure_table(
                                                    &conn,
                                                    sheet_name,
                                                    &old_name,
                                                    new_name,
                                                ) {
                                                    Ok(_) => info!("Successfully renamed structure table in DB"),
                                                    Err(e) => error!("Failed to rename structure table in DB: {}", e),
                                                }
                                            }
                                            // Rename data column if it exists physically (non-structure)
                                            if !was_structure_before {
                                                debug!("Renaming data column in main table");
                                                match crate::sheets::database::writer::DbWriter::rename_data_column(
                                                    &conn,
                                                    sheet_name,
                                                    &old_name,
                                                    new_name,
                                                ) {
                                                    Ok(_) => info!("Successfully renamed column in DB"),
                                                    Err(e) => error!("Failed to rename column in DB: {}", e),
                                                }
                                            } else {
                                                // Update metadata column_name for the parent (structure columns still have metadata)
                                                debug!("Updating metadata column name");
                                                match crate::sheets::database::writer::DbWriter::update_metadata_column_name(
                                                    &conn,
                                                    sheet_name,
                                                    col_index,
                                                    new_name,
                                                ) {
                                                    Ok(_) => info!("Successfully updated metadata column name in DB"),
                                                    Err(e) => error!("Failed to update metadata column name in DB: {}", e),
                                                }
                                            }
                                        }
                                        
                                        // Cascade column rename to child tables (update parent_key and grand_N_parent values)
                                        // This ensures filtering continues to work after column rename
                                        debug!("Cascading column rename to child structure tables");
                                        match crate::sheets::database::writer::DbWriter::cascade_column_rename_to_children(
                                            &conn,
                                            sheet_name,
                                            &old_name,
                                            new_name,
                                        ) {
                                            Ok(_) => info!("Successfully cascaded column rename to children"),
                                            Err(e) => error!("Failed to cascade column rename: {}", e),
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to open DB connection for column rename: {}", e);
                                    }
                                }
                            } else {
                                warn!("DB file not found for column rename: {:?}", db_path);
                            }
                        }
                        success = true;
                        // Emit data modified event so virtual structure sheets trigger parent schema sync
                        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
                            category: category.clone(),
                            sheet_name: sheet_name.clone(),
                        });
                        metadata_cache = Some(metadata.clone()); // Cache metadata for saving
                    }
                } else {
                    // --- CORRECTED: Use columns.len() in error message ---
                    feedback_writer.write(SheetOperationFeedback {
                        message: format!(
                            "Failed column rename in '{:?}/{}': Index {} out of bounds ({} columns).",
                            category,
                            sheet_name,
                            col_index,
                            metadata.columns.len() // Use columns.len()
                        ),
                        is_error: true,
                    });
                }
            } else {
                feedback_writer.write(SheetOperationFeedback {
                    message: format!(
                        "Failed column rename in '{:?}/{}': Metadata missing.",
                        category, sheet_name
                    ),
                    is_error: true,
                });
            }
        } else {
            feedback_writer.write(SheetOperationFeedback {
                message: format!(
                    "Failed column rename: Sheet '{:?}/{}' not found.",
                    category, sheet_name
                ),
                is_error: true,
            });
        }

        // If successful, mark sheet for saving
        if success {
            if let Some(meta) = metadata_cache {
                let key = (category.clone(), sheet_name.clone());
                changed_sheets.insert(key, meta);
            }
        }
    } // End event loop

    // --- Trigger Saves (Immutable Borrow) ---
    if !changed_sheets.is_empty() {
        let registry_immut = registry.as_ref(); // Get immutable borrow for saving
        for ((cat, name), metadata) in changed_sheets {
            info!(
                "Column name updated for '{:?}/{}', triggering save.",
                cat, name
            );
            save_single_sheet(registry_immut, &metadata); // Pass metadata
        }
    }
}
