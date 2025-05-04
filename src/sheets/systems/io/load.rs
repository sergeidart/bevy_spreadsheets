// src/sheets/systems/io/load.rs
use bevy::prelude::*;
use std::{
    fs::{self, File},
    io::{self, BufReader},
    path::{Path, PathBuf}, // Ensure Path and PathBuf are imported
};
use super::save::save_single_sheet; // CHANGED IMPORT
use super::{DEFAULT_DATA_DIR, get_default_data_base_path}; // Ensure this is imported
use crate::sheets::{
    definitions::{SheetGridData, SheetMetadata},
    // Import relevant events, including new ones for upload flow and feedback
    events::{
        JsonSheetUploaded, RequestInitiateFileUpload, RequestProcessUpload,
        SheetOperationFeedback
    },
    resources::SheetRegistry,
};
// Import metadata creation functions
use crate::example_definitions::{create_example_items_metadata, create_simple_config_metadata}; //


/// Helper function to load and parse a JSON file expected to contain Vec<Vec<String>>.
/// Takes &Path and returns Result<(Vec<Vec<String>>, String), String> where String is error message.
pub fn load_and_parse_json_sheet(path: &Path) -> Result<(Vec<Vec<String>>, String), String> {
     let file_content = fs::read_to_string(path)
         .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;

     // Trim potential BOM (Byte Order Mark) which can cause JSON parsing errors
     let trimmed_content = file_content.trim_start_matches('\u{FEFF}');

     if trimmed_content.is_empty() {
         // Handle empty file gracefully - return empty grid
         warn!("File '{}' is empty. Loading as empty sheet.", path.display());
         // Still return the filename derived from the path
         return Ok((Vec::new(), path.file_name().map_or("unknown.json".to_string(), |s| s.to_string_lossy().into_owned())));
     }

     let grid: Vec<Vec<String>> = serde_json::from_str(trimmed_content)
         .map_err(|e| format!("Failed to parse JSON from '{}' (expected array of arrays of strings): {}", path.display(), e))?;

     // Get the filename from the path
     let filename = path.file_name()
         .map(|s| s.to_string_lossy().into_owned())
         .unwrap_or_else(|| "unknown.json".to_string()); // Fallback filename

     Ok((grid, filename))
 }


/// Startup system to register template sheet metadata *only if the data directory doesn't exist*.
/// Must run before loading systems.
pub fn register_sheet_metadata(mut registry: ResMut<SheetRegistry>) { //
    let data_dir_path = get_default_data_base_path(); //

    if !data_dir_path.exists() {
        info!(
            "Data directory '{:?}' does not exist. Registering default template sheets.",
            data_dir_path
        );
        let registered_example = registry.register(create_example_items_metadata()); //
        let registered_config = registry.register(create_simple_config_metadata()); //
        if registered_example || registered_config {
             info!("Registered pre-defined template sheet metadata.");
        }
    } else {
         info!(
            "Data directory '{:?}' already exists. Skipping registration of default template sheets.",
            data_dir_path
        );
    }
    // --- MODIFICATION END ---
}

/// Startup system to load data ONLY for already registered sheets from JSON files.
/// Assumes metadata has already been registered. Does NOT save after load.
pub fn load_registered_sheets_startup(mut registry: ResMut<SheetRegistry>) { //
    // ... (rest of the function remains the same)
    info!("Loading data for registered sheets...");
    let base_path = get_default_data_base_path();

    if !base_path.exists() {
        info!("Data directory '{:?}' does not exist yet. Skipping load for registered sheets.", base_path);
        return;
    }

    let sheets_to_load: Vec<(String, String)> = registry
        .iter_sheets()
        .filter_map(|(name, data)| {
            data.metadata.as_ref().map(|m| (name.clone(), m.data_filename.clone()))
        })
        .collect();

    if sheets_to_load.is_empty() {
        info!("No pre-registered sheets with filenames found to load.");
        return;
    }

    for (sheet_name, filename_to_load) in sheets_to_load {
        let full_path = base_path.join(&filename_to_load);
        trace!("Attempting load for registered sheet '{}' from '{}'...", sheet_name, full_path.display());

        // Use the updated helper function which takes &Path
        match load_and_parse_json_sheet(&full_path) {
            Ok((grid_data, _)) => {
                if let Some(sheet_entry) = registry.get_sheet_mut(&sheet_name) {
                     let expected_cols = sheet_entry.metadata.as_ref().map_or(0, |m| m.column_headers.len());
                     let loaded_cols = grid_data.first().map_or(0, |row| row.len());
                     if expected_cols > 0 && !grid_data.is_empty() && loaded_cols != expected_cols {
                         warn!(
                             "Sheet '{}': Loaded grid columns ({}) mismatch metadata ({}).",
                             sheet_name, loaded_cols, expected_cols
                         );
                     }
                     sheet_entry.grid = grid_data;
                     info!("Successfully loaded {} rows for registered sheet '{}'.", sheet_entry.grid.len(), sheet_name);
                } else { error!("Registered sheet '{}' disappeared during load.", sheet_name); }
            }
            Err(e) => {
                 if let Some(sheet_entry) = registry.get_sheet_mut(&sheet_name) {
                      if !sheet_entry.grid.is_empty() { sheet_entry.grid.clear(); }
                 }
                 if e.contains("Failed to read file") && (e.contains("os error 2") || e.contains("os error 3") || e.contains("system cannot find the file specified")) {
                     info!("Data file '{}' not found for registered sheet '{}'.", filename_to_load, sheet_name);
                 } else if !e.contains("File is empty") {
                     error!("Failed to load registered sheet '{}' from '{}': {}", sheet_name, filename_to_load, e);
                 }
            }
        }
    }
    info!("Finished loading data for registered sheets.");
    // No save needed here - this is initial state loading.
}


/// Handles the `JsonSheetUploaded` event. Triggers save for the specific sheet on success.
pub fn handle_json_sheet_upload(
    // ... (function remains the same)
    mut events: EventReader<JsonSheetUploaded>,
    mut registry: ResMut<SheetRegistry>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
) {
    // No need for 'changed' flag, save inside the loop
    for event in events.read() {
        info!("Processing uploaded sheet event for '{}' from file '{}'...", event.desired_sheet_name, event.original_filename);

        let sheet_name = event.desired_sheet_name.clone();

        // Check for name collisions before adding - Send feedback if overwriting
        if registry.get_sheet(&sheet_name).is_some() {
            let msg = format!("Sheet name '{}' already exists. Overwriting with uploaded data.", sheet_name);
            warn!("{}", msg);
            feedback_writer.send(SheetOperationFeedback { message: msg, is_error: false });
        }

        // Create SheetGridData - metadata will be created/updated in add_or_replace_sheet
        let mut sheet_data = SheetGridData {
             metadata: None, // Let add_or_replace handle it initially
             grid: event.grid_data.clone(),
        };

        // Ensure metadata reflects the uploaded filename and generate if needed
        let num_cols = sheet_data.grid.first().map_or(0, |row| row.len());
        let generated_metadata = SheetMetadata::create_generic(
            sheet_name.clone(),
            event.original_filename.clone(), // Use original filename from event
            num_cols
        );
        sheet_data.metadata = Some(generated_metadata); // Set the correct metadata

        registry.add_or_replace_sheet(sheet_name.clone(), sheet_data);

        let msg = format!("Successfully loaded and registered sheet '{}' from upload.", sheet_name);
        info!("{}", msg); // Keep internal log
        feedback_writer.send(SheetOperationFeedback { message: msg, is_error: false });

        // Save the specific sheet immediately
        info!("Sheet '{}' uploaded/replaced, triggering immediate save.", sheet_name);
        save_single_sheet(&registry, &sheet_name); // MODIFIED CALL
    } // End event loop
}


pub fn handle_initiate_file_upload(
    mut events: EventReader<RequestInitiateFileUpload>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut process_event_writer: EventWriter<RequestProcessUpload>,
) {
    if !events.is_empty() {
        events.clear(); // Consume the event(s)
        info!("File upload initiated by UI.");

        // Use blocking file dialog (appropriate for typical Bevy system)
        let picked_file: Option<PathBuf> = rfd::FileDialog::new()
            .add_filter("JSON files", &["json"])
            .pick_file();

        match picked_file {
            Some(path) => {
                info!("File picked: '{}'. Sending request to process.", path.display());
                process_event_writer.send(RequestProcessUpload { path });
            }
            None => {
                 let msg = "File selection cancelled.".to_string();
                 info!("{}", msg);
                 feedback_writer.send(SheetOperationFeedback { message: msg, is_error: false });
            }
        }
    }
}

pub fn handle_process_upload_request(
     mut events: EventReader<RequestProcessUpload>,
    mut feedback_writer: EventWriter<SheetOperationFeedback>,
    mut uploaded_event_writer: EventWriter<JsonSheetUploaded>,
) {
     for event in events.read() {
         let path = &event.path;
         let filename_display = path.file_name()
            .map_or_else(|| path.display().to_string(), |os| os.to_string_lossy().into_owned());

         info!("Processing uploaded file request for: '{}'", path.display());

         match load_and_parse_json_sheet(path) {
             Ok((grid_data, original_filename)) => {
                 let desired_name = path.file_stem()
                                      .map(|s| s.to_string_lossy().into_owned())
                                      .unwrap_or_else(|| original_filename.trim_end_matches(".json").trim_end_matches(".JSON").to_string());

                 if desired_name.is_empty() {
                     let msg = format!("Upload failed for '{}': Could not determine sheet name from filename.", filename_display);
                      error!("{}", msg);
                      feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
                 } else {
                     // Send the successful upload data to be handled by handle_json_sheet_upload
                     uploaded_event_writer.send(JsonSheetUploaded {
                         desired_sheet_name: desired_name,
                         original_filename: original_filename,
                         grid_data: grid_data,
                     });
                 }
             }
             Err(e) => {
                  let msg = format!("Error processing file '{}': {}", filename_display, e);
                  error!("{}", msg);
                  feedback_writer.send(SheetOperationFeedback { message: msg, is_error: true });
             }
         }
     }
}