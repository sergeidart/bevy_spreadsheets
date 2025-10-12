// src/sheets/systems/io/load.rs
// Handles runtime uploads

use bevy::prelude::*;
use std::path::PathBuf;

// Use items from sibling modules
use super::parsers;
use super::save::save_single_sheet;
use super::validator;
// get_default_data_base_path is not directly needed by these handlers

// Use types and events from the main 'sheets' module
use crate::sheets::{
    definitions::{SheetGridData, SheetMetadata},
    events::{
        JsonSheetUploaded,
        RequestInitiateFileUpload,
        RequestProcessUpload,
        SheetDataModifiedInRegistryEvent, // Added SheetDataModifiedInRegistryEvent
        SheetOperationFeedback,
    },
    resources::SheetRegistry,
};

/// Handles the `JsonSheetUploaded` event. (Sent by handle_process_upload_request)
/// Adds the uploaded sheet to the specified category in the registry.
pub fn handle_json_sheet_upload(
    mut events: EventReader<JsonSheetUploaded>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut data_modified_writer: EventWriter<SheetDataModifiedInRegistryEvent>, // Added writer
) {
    for event in events.read() {
        let category = &event.category;
        let sheet_name = &event.desired_sheet_name;
        info!(
            "Registering/Updating sheet '{:?}/{}' from uploaded file '{}'...",
            category, sheet_name, event.original_filename
        );

        let num_cols = event.grid_data.first().map_or(0, |row| row.len());
        let generated_metadata = SheetMetadata::create_generic(
            sheet_name.clone(),
            event.original_filename.clone(),
            num_cols,
            category.clone(),
        );

        if let Err(e) =
            validator::validate_grid_structure(&event.grid_data, &generated_metadata, sheet_name)
        {
            error!(
                "Internal Error: Grid validation failed for generated metadata: {}",
                e
            );
            feedback_writer.write(SheetOperationFeedback {
                message: format!("Internal error during upload for '{}'", sheet_name),
                is_error: true,
            });
            continue;
        }

        let sheet_data = SheetGridData {
            metadata: Some(generated_metadata.clone()),
            grid: event.grid_data.clone(),
            row_indices: Vec::new(), // JSON-backed sheets don't track row_indices
        };

        registry.add_or_replace_sheet(category.clone(), sheet_name.clone(), sheet_data);
        let msg = format!(
            "Sheet '{:?}/{}' successfully uploaded and registered.",
            category, sheet_name
        );
        info!("{}", msg);
        feedback_writer.write(SheetOperationFeedback {
            message: msg,
            is_error: false,
        });

        // Send data modified event
        data_modified_writer.write(SheetDataModifiedInRegistryEvent {
            category: category.clone(),
            sheet_name: sheet_name.clone(),
        });

        info!(
            "Triggering immediate save for uploaded sheet '{:?}/{}'.",
            category, sheet_name
        );
        save_single_sheet(&registry, &generated_metadata);
    }
}

/// Handles the UI request to initiate a file upload dialog.
pub fn handle_initiate_file_upload(
    mut events: EventReader<RequestInitiateFileUpload>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut process_event_writer: EventWriter<RequestProcessUpload>,
) {
    if !events.is_empty() {
        events.clear();
        info!("File upload initiated by UI.");
        let picked_file: Option<PathBuf> = rfd::FileDialog::new()
            .add_filter("JSON Grid Data files", &["json"])
            .pick_file();
        match picked_file {
            Some(path) => {
                if path
                    .file_name()
                    .map_or(false, |name| name.to_string_lossy().ends_with(".meta.json"))
                {
                    let msg = format!("Warning: Selected file '{}' looks like a metadata file. Please select the main data (.json) file.", path.display());
                    warn!("{}", msg);
                    feedback_writer.write(SheetOperationFeedback {
                        message: msg,
                        is_error: true,
                    });
                } else {
                    info!(
                        "File picked: '{}'. Sending request to process.",
                        path.display()
                    );
                    process_event_writer.write(RequestProcessUpload { path });
                }
            }
            None => {
                let msg = "File selection cancelled.".to_string();
                info!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: false,
                });
            }
        }
    }
}

/// Handles the request to process a file path selected via the upload dialog.
pub fn handle_process_upload_request(
    mut events: EventReader<RequestProcessUpload>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut uploaded_event_writer: EventWriter<JsonSheetUploaded>,
    registry: Res<SheetRegistry>,
) {
    for event in events.read() {
        let path = &event.path;
        let filename_display = path.file_name().map_or_else(
            || path.display().to_string(),
            |os| os.to_string_lossy().into_owned(),
        );
        let filename_only = path
            .file_name()
            .map(|os| os.to_string_lossy().into_owned())
            .unwrap_or_default();

        info!("Processing uploaded file request for: '{}'", path.display());

        if let Err(e) = validator::validate_file_exists(path) {
            error!("Upload validation failed for '{}': {}", filename_display, e);
            feedback_writer.write(SheetOperationFeedback {
                message: e,
                is_error: true,
            });
            continue;
        }
        let derived_name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                filename_display
                    .trim_end_matches(".json")
                    .trim_end_matches(".JSON")
                    .to_string()
            });

        if let Err(e) = validator::validate_sheet_name_for_upload(
            &derived_name,
            &registry,
            &mut feedback_writer,
        ) {
            let msg = format!("Upload failed for '{}': {}", filename_display, e);
            error!("{}", msg);
            feedback_writer.write(SheetOperationFeedback {
                message: msg,
                is_error: true,
            });
            continue;
        }

        match parsers::read_and_parse_json_sheet(path) {
            Ok((grid_data, _)) => {
                uploaded_event_writer.write(JsonSheetUploaded {
                    category: None,
                    desired_sheet_name: derived_name,
                    original_filename: filename_only,
                    grid_data: grid_data,
                });
            }
            Err(e) => {
                let msg = format!("Error processing file '{}': {}", filename_display, e);
                error!("{}", msg);
                feedback_writer.write(SheetOperationFeedback {
                    message: msg,
                    is_error: true,
                });
            }
        }
    }
}
