// src/sheets/systems/logic/delete_sheet.rs
use crate::sheets::{
    definitions::SheetMetadata, // Needed for path generation
    events::{RequestDeleteSheet, RequestDeleteSheetFile, SheetOperationFeedback},
    resources::SheetRegistry,
};
use bevy::prelude::*;
use std::path::PathBuf; // Added for relative path

/// Handles requests to delete a sheet from the registry and requests deletion of associated files.
pub fn handle_delete_request(
    mut events: EventReader<RequestDeleteSheet>,
    mut registry: ResMut<SheetRegistry>,
    mut file_delete_writer: EventWriter<RequestDeleteSheetFile>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<crate::sheets::events::SheetDataModifiedInRegistryEvent>,
) {
    for event in events.read() {
        let category = &event.category; // <<< Get category
        let sheet_name = &event.sheet_name;
        info!(
            "Handling delete request for sheet: '{:?}/{}'",
            category, sheet_name
        );

        // --- Get metadata BEFORE attempting delete ---
        // Need immutable borrow first to clone metadata if sheet exists
        let metadata_opt: Option<SheetMetadata> = {
            let registry_immut = registry.as_ref();
            registry_immut
                .get_sheet(category, sheet_name)
                .and_then(|d| d.metadata.clone()) // Clone metadata if present
        };

        // Check if sheet exists before attempting delete (using immutable borrow again)
        if metadata_opt.is_none() {
            let msg = format!(
                "Delete failed: Sheet '{:?}/{}' not found or missing metadata.",
                category, sheet_name
            );
            error!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
            continue; // Skip to next event
        }

        // --- Perform Delete in Registry (Mutable Borrow) ---
        // Use the category from the event
        match registry.delete_sheet(category, sheet_name) {
            Ok(removed_data) => {
                // Registry deletion returns the removed data
                let msg = format!(
                    "Successfully deleted sheet '{:?}/{}' from registry.",
                    category, sheet_name
                );
                info!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: false,
                });

                // Notify that sheet data changed so UI and caches can respond (clears transient UI feedback)
                data_modified_writer.write(
                    crate::sheets::events::SheetDataModifiedInRegistryEvent {
                        category: category.clone(),
                        sheet_name: sheet_name.clone(),
                    },
                );

                // --- Request File Deletions (using metadata from removed data) ---
                if let Some(metadata) = removed_data.metadata {
                    // Use metadata from the returned data
                    // Construct relative paths
                    let mut grid_relative_path = PathBuf::new();
                    if let Some(cat) = &metadata.category {
                        grid_relative_path.push(cat);
                    }
                    grid_relative_path.push(&metadata.data_filename);

                    let mut meta_relative_path = PathBuf::new();
                    if let Some(cat) = &metadata.category {
                        meta_relative_path.push(cat);
                    }
                    meta_relative_path.push(format!("{}.meta.json", metadata.sheet_name));

                    if !metadata.data_filename.is_empty() {
                        info!(
                            "Requesting grid file deletion: '{}'",
                            grid_relative_path.display()
                        );
                        file_delete_writer.write(RequestDeleteSheetFile {
                            relative_path: grid_relative_path,
                        });
                    } else {
                        warn!(
                            "No grid filename found in metadata for deleted sheet '{:?}/{}'.",
                            category, metadata.sheet_name
                        );
                    }
                    info!(
                        "Requesting meta file deletion: '{}'",
                        meta_relative_path.display()
                    );
                    file_delete_writer.write(RequestDeleteSheetFile {
                        relative_path: meta_relative_path,
                    });
                } else {
                    // This case should ideally not happen if metadata_opt check passed
                    warn!("Cannot request file deletion for '{:?}/{}': Metadata was missing in removed data.", category, sheet_name);
                }
            }
            Err(e) => {
                // Error from registry.delete_sheet
                let msg = format!(
                    "Failed to delete sheet '{:?}/{}' from registry: {}",
                    category, sheet_name, e
                );
                error!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        }
    }
}
