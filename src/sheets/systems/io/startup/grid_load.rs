// src/sheets/systems/io/startup/grid_load.rs
use crate::sheets::systems::io::{parsers::read_and_parse_json_sheet, validator}; // Added validator
use bevy::prelude::*;
use std::path::Path;

/// Loads grid data (Vec<Vec<String>>) from a specified JSON file path.
/// Performs basic file existence and type validation.
///
/// # Arguments
/// * `grid_path` - The full path to the `.json` grid data file.
///
/// # Returns
/// * `Ok(Some(Vec<Vec<String>>))` - If the file exists, is not empty, and parses correctly.
/// * `Ok(None)` - If the file exists but is empty or contains only empty rows.
/// * `Err(String)` - If the file doesn't exist, is not a file, or fails to parse.
pub(super) fn load_grid_data_file(
    grid_path: &Path,
) -> Result<Option<Vec<Vec<String>>>, String> {
    // --- File Validation ---
    // Use validator::validate_file_exists which checks existence and is_file
    if let Err(e) = validator::validate_file_exists(grid_path) {
        // It's an error if the expected grid file doesn't exist or isn't a file
        return Err(e);
    }

    // --- Parsing ---
    match read_and_parse_json_sheet(grid_path) {
        Ok((grid, _filename)) => {
            // Check if grid is effectively empty (either `[]` or `[[], [], ...]`)
            if grid.is_empty() || grid.iter().all(|row| row.is_empty()) {
                trace!(
                    "Grid file '{}' is empty or contains only empty rows.",
                    grid_path.display()
                );
                Ok(None) // Treat as empty
            } else {
                trace!(
                    "Loaded grid data with {} rows from '{}'.",
                    grid.len(),
                    grid_path.display()
                );
                Ok(Some(grid)) // Return the loaded grid
            }
        }
        Err(e) => {
            // Check if the error indicates an empty file according to the parser
            if e.to_lowercase().contains("file is empty") {
                trace!(
                    "Grid file '{}' reported as empty by parser.",
                    grid_path.display()
                );
                Ok(None) // Treat as empty
            } else {
                // Return other parsing errors
                Err(format!(
                    "Failed to parse grid data from '{}': {}",
                    grid_path.display(),
                    e
                ))
            }
        }
    }
}

// Potential future function (if needed for loading registered sheets differently)
// pub(super) fn load_grid_for_registered_sheet(...) -> Result<Option<Vec<Vec<String>>>, String> { ... }
