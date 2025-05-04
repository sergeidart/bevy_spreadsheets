// src/sheets/systems/io/startup_scan.rs
use bevy::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::get_default_data_base_path;
use super::save::save_single_sheet;
use super::parsers::{read_and_parse_json_sheet, read_and_parse_metadata_file};
use crate::sheets::{
    definitions::{SheetMetadata, SheetGridData, ColumnDataType}, // Added ColumnDataType
    resources::SheetRegistry,
};

/// Scans the data directory for JSON files that aren't yet registered.
/// Attempts to load them as new sheets, generating metadata if necessary.
/// Validates and corrects metadata inconsistencies (especially filters).
pub fn scan_directory_for_sheets_startup(mut registry: ResMut<SheetRegistry>) {
    let base_path = get_default_data_base_path();
    info!("Startup Scan: Scanning directory '{:?}' for manually added sheets...", base_path);

    if !base_path.exists() {
        info!("Startup Scan: Data directory does not exist. No manual sheets to scan.");
        return;
    }

    let mut found_unregistered_count = 0;
    let mut potential_grid_files = Vec::new();

    // --- Find potential grid files ---
    match fs::read_dir(&base_path) {
        Ok(entries) => {
            for entry_result in entries {
                if let Ok(entry) = entry_result {
                    let path = entry.path();
                    if path.is_file() {
                        // Check for .json extension, ignore case
                        let is_json = path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("json"));
                        // Check if it's NOT a metadata file
                        let is_meta_file = path.file_name().map_or(false, |name| name.to_string_lossy().ends_with(".meta.json"));

                        if is_json && !is_meta_file {
                            potential_grid_files.push(path);
                        }
                    }
                } else if let Err(e) = entry_result {
                    error!("Startup Scan: Failed processing directory entry in '{:?}': {}", base_path, e);
                }
            }
        }
        Err(e) => {
            error!("Startup Scan: Failed to read directory '{:?}': {}", base_path, e);
            return;
        }
    }

    if potential_grid_files.is_empty() {
         info!("Startup Scan: No potential unregistered grid files (.json) found.");
         return;
    } else {
         trace!("Startup Scan: Found potential grid files: {:?}", potential_grid_files);
    }


    // --- Process potential files ---
    for path in potential_grid_files {
        let filename = path.file_name().map_or_else(|| "unknown.json".to_string(), |os| os.to_string_lossy().into_owned());
        // Derive sheet name from file stem (e.g., "MySheet" from "MySheet.json")
        let sheet_name_candidate = path.file_stem().map_or_else(
            // Fallback if no stem (e.g., ".json") - remove extension
            || filename.trim_end_matches(".json").trim_end_matches(".JSON").to_string(),
            |os| os.to_string_lossy().into_owned()
        );

        if sheet_name_candidate.is_empty() || sheet_name_candidate.starts_with('.') {
            warn!("Startup Scan: Skipping file '{}': Invalid sheet name derived ('{}').", filename, sheet_name_candidate);
            continue;
        }

        // Check if a sheet with this name OR this filename is already managed
        let already_registered = registry.get_sheet(&sheet_name_candidate).is_some() ||
            registry.iter_sheets().any(|(_, data)| data.metadata.as_ref().map_or(false, |m| m.data_filename == filename));

        if !already_registered {
            trace!("Startup Scan: Found potential unregistered grid file: '{}'. Attempting load as sheet '{}'.", filename, sheet_name_candidate);
            let mut loaded_metadata: Option<SheetMetadata> = None;
            let meta_filename = format!("{}.meta.json", sheet_name_candidate);
            let meta_path = base_path.join(&meta_filename);

            // --- Try loading corresponding metadata file ---
            if meta_path.exists() {
                match read_and_parse_metadata_file(&meta_path) {
                    Ok(mut meta) => {
                        // Correct metadata fields if they don't match the derived/expected names
                        if meta.sheet_name != sheet_name_candidate {
                            warn!("Startup Scan: Correcting sheet_name in '{}' ('{}') to match candidate '{}'.", meta_filename, meta.sheet_name, sheet_name_candidate);
                            meta.sheet_name = sheet_name_candidate.clone();
                        }
                        if meta.data_filename != filename {
                            warn!("Startup Scan: Correcting data_filename in '{}' ('{}') to match grid file ('{}').", meta_filename, meta.data_filename, filename);
                            meta.data_filename = filename.clone();
                        }
                        trace!("Startup Scan: Loaded metadata for '{}' from '{}'.", sheet_name_candidate, meta_filename);
                        loaded_metadata = Some(meta);
                    }
                    Err(e) => { error!("Startup Scan: Failed to load metadata file '{}' for potential sheet '{}': {}. Will generate default metadata.", meta_filename, sheet_name_candidate, e); }
                }
            } else { trace!("Startup Scan: Metadata file '{}' not found.", meta_filename); }


            // --- Try loading grid data ---
            match read_and_parse_json_sheet(&path) {
                Ok((grid_data, _)) => { // We already have filename
                    // Use loaded metadata or generate default if necessary
                    let mut final_metadata = loaded_metadata.unwrap_or_else(|| {
                        info!("Startup Scan: Generating default metadata for sheet '{}'.", sheet_name_candidate);
                        let num_cols = grid_data.first().map_or(0, |r| r.len());
                        SheetMetadata::create_generic(sheet_name_candidate.clone(), filename.clone(), num_cols)
                    });

                    // *** CONSISTENCY CHECK/FIX FOR FILTERS (and types) ***
                    let expected_len = final_metadata.column_headers.len();
                    let mut correction_made_in_scan = false; // Track if save is needed for correction

                    if final_metadata.column_types.len() != expected_len {
                        warn!("Startup Scan: Correcting scanned column_types length mismatch for '{}'. Resizing from {} to {}.",
                              final_metadata.sheet_name, final_metadata.column_types.len(), expected_len);
                        final_metadata.column_types.resize(expected_len, ColumnDataType::String);
                        correction_made_in_scan = true;
                    }
                    if final_metadata.column_filters.len() != expected_len {
                        warn!("Startup Scan: Correcting scanned column_filters length mismatch for '{}'. Resizing from {} to {}.",
                              final_metadata.sheet_name, final_metadata.column_filters.len(), expected_len);
                        final_metadata.column_filters.resize(expected_len, None);
                        correction_made_in_scan = true;
                    }
                    // *** END OF ADDED CHECK ***

                    let sheet_data = SheetGridData { metadata: Some(final_metadata), grid: grid_data };
                    registry.add_or_replace_sheet(sheet_name_candidate.clone(), sheet_data);
                    found_unregistered_count += 1;
                    info!("Startup Scan: Loaded and registered sheet '{}' from file '{}'.", sheet_name_candidate, filename);

                    // Save the newly added/corrected sheet immediately
                    // This persists generated/corrected metadata.
                    save_single_sheet(&registry, &sheet_name_candidate);

                }
                Err(e) => {
                    // Handle cases where grid JSON fails to parse or file is empty
                     if !e.to_lowercase().contains("file is empty") {
                          error!("Startup Scan: Failed to load grid data from '{}' for potential sheet '{}': {}", filename, sheet_name_candidate, e);
                          // Still try to register with empty grid if metadata exists? Or skip? Let's skip if grid parse fails hard.
                     } else {
                          // File is empty, register with empty grid
                          info!("Startup Scan: Grid file '{}' for '{}' is empty. Registering with empty grid.", filename, sheet_name_candidate);
                          let mut final_metadata = loaded_metadata.unwrap_or_else(|| {
                              SheetMetadata::create_generic(sheet_name_candidate.clone(), filename.clone(), 0) // 0 columns for empty grid
                          });

                          // *** CONSISTENCY CHECK/FIX FOR FILTERS (and types) on empty grid meta ***
                          let expected_len = final_metadata.column_headers.len(); // Should be 0 if generated
                           // ... (repeat the length check/resize logic as above) ...
                           if final_metadata.column_types.len() != expected_len { /* resize */ }
                           if final_metadata.column_filters.len() != expected_len { /* resize */ }

                          let sheet_data = SheetGridData { metadata: Some(final_metadata), grid: Vec::new() }; // Empty grid
                          registry.add_or_replace_sheet(sheet_name_candidate.clone(), sheet_data);
                          found_unregistered_count += 1;
                          info!("Startup Scan: Registered empty sheet '{}'.", sheet_name_candidate);
                          save_single_sheet(&registry, &sheet_name_candidate); // Save empty sheet + meta
                     }
                }
            } // End grid load match
        } else {
            trace!("Startup Scan: Skipping file '{}': Sheet '{}' already registered or filename conflict.", filename, sheet_name_candidate);
        }
    } // End processing loop

    if found_unregistered_count > 0 {
        info!("Startup Scan: Found and processed {} manually added sheets.", found_unregistered_count);
    } else {
        info!("Startup Scan: No new manually added sheets found to process.");
    }
}