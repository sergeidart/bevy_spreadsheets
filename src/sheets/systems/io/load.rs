// src/sheets/systems/io/load.rs

use bevy::prelude::*;
use std::{
    fs::{self, File},
    io::{self, BufReader},
    path::PathBuf,
};

// Use items defined in the parent io module (io/mod.rs)
use super::{DEFAULT_DATA_DIR, get_default_data_base_path};

// Use types from the main sheets module
use crate::sheets::{
    definitions::{SheetGridData, SheetMetadata},
    events::JsonSheetUploaded,
    resources::SheetRegistry,
};
// Import metadata creation functions
use crate::example_definitions::{create_example_items_metadata, create_simple_config_metadata};


/// Helper function to load and parse a JSON file expected to contain Vec<Vec<String>>.
/// (Moved here from the original io.rs)
pub fn load_and_parse_json_sheet(path: &PathBuf) -> Result<(Vec<Vec<String>>, String), String> {
     let file_content = fs::read_to_string(path)
         .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;

     // Trim potential BOM (Byte Order Mark) which can cause JSON parsing errors
     let trimmed_content = file_content.trim_start_matches('\u{FEFF}');

     if trimmed_content.is_empty() {
         // Handle empty file gracefully - return empty grid
         warn!("File '{}' is empty. Loading as empty sheet.", path.display());
         return Ok((Vec::new(), path.file_name().map_or("unknown.json".to_string(), |s| s.to_string_lossy().into_owned())));
     }

     let grid: Vec<Vec<String>> = serde_json::from_str(trimmed_content)
         .map_err(|e| format!("Failed to parse JSON from '{}' (expected array of arrays of strings): {}", path.display(), e))?;

     let filename = path.file_name()
         .map(|s| s.to_string_lossy().into_owned())
         .unwrap_or_else(|| "unknown.json".to_string());

     Ok((grid, filename))
 }


/// Startup system to register all known sheet metadata for *this app*.
/// Must run before loading systems.
pub fn register_sheet_metadata(mut registry: ResMut<SheetRegistry>) {
    let registered_example = registry.register(create_example_items_metadata());
    let registered_config = registry.register(create_simple_config_metadata());
    if registered_example || registered_config {
        info!("Registered pre-defined sheet metadata.");
    }
}

/// Startup system to load data ONLY for already registered sheets from JSON files.
/// Assumes metadata has already been registered.
pub fn load_registered_sheets_startup(mut registry: ResMut<SheetRegistry>) {
    info!("Loading data for registered sheets...");
    let base_path = get_default_data_base_path();

    // Ensure the base directory exists (though scanning might create it too)
     if !base_path.exists() {
        info!("Data directory '{:?}' does not exist yet. Skipping load for registered sheets.", base_path);
        // Optionally create it here: fs::create_dir_all(&base_path).ok();
        return;
    }

    // Collect names and filenames first to avoid borrowing issues
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
        // Use trace level for attempting load, info for success/specific errors
        trace!("Attempting load for registered sheet '{}' from '{}'...", sheet_name, full_path.display());

        match load_and_parse_json_sheet(&full_path) {
            Ok((grid_data, _)) => {
                if let Some(sheet_entry) = registry.get_sheet_mut(&sheet_name) {
                     // Optional validation
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
                 // Use contains("read file") and os error codes for better cross-platform checks
                 if e.contains("Failed to read file") && (e.contains("os error 2") || e.contains("os error 3") || e.contains("system cannot find the file specified")) {
                     info!("Data file '{}' not found for registered sheet '{}'.", filename_to_load, sheet_name);
                 } else if !e.contains("File is empty") { // Don't log error for handled empty files
                     error!("Failed to load registered sheet '{}' from '{}': {}", sheet_name, filename_to_load, e);
                 }
            }
        }
    }
    info!("Finished loading data for registered sheets.");
}


/// Handles the `JsonSheetUploaded` event. (Update system)
pub fn handle_json_sheet_upload(
    mut events: EventReader<JsonSheetUploaded>,
    mut registry: ResMut<SheetRegistry>,
) {
    for event in events.read() {
        info!("Processing uploaded sheet event for '{}' from file '{}'...", event.desired_sheet_name, event.original_filename);

        // Check for name collisions before adding
        if registry.get_sheet(&event.desired_sheet_name).is_some() {
            warn!("Sheet name '{}' from upload event already exists. Overwriting.", event.desired_sheet_name);
        }

        // Create SheetGridData - metadata will be created/updated in add_or_replace_sheet
        let mut sheet_data = SheetGridData {
             metadata: None, // Let add_or_replace handle it initially
             grid: event.grid_data.clone(),
        };

        // Ensure metadata reflects the uploaded filename and generate if needed
        let num_cols = sheet_data.grid.first().map_or(0, |row| row.len());
        let generated_metadata = SheetMetadata::create_generic(
            event.desired_sheet_name.clone(),
            event.original_filename.clone(), // Use original filename
            num_cols
        );
        sheet_data.metadata = Some(generated_metadata); // Set the correct metadata

        // Add/replace in registry
        registry.add_or_replace_sheet(event.desired_sheet_name.clone(), sheet_data);

        info!("Successfully processed upload event for sheet '{}'.", event.desired_sheet_name);
    }
}