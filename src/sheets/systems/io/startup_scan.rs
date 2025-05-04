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
    definitions::{SheetMetadata, SheetGridData}, // Removed unused ColumnDataType import
    resources::SheetRegistry,
};

pub fn scan_directory_for_sheets_startup(mut registry: ResMut<SheetRegistry>) {
    let base_path = get_default_data_base_path();
    info!("Startup Scan: Scanning directory '{:?}' for manually added sheets...", base_path);

    if !base_path.exists() { info!("Startup Scan: Data directory does not exist."); return; }

    let mut found_unregistered_count = 0;
    let mut potential_grid_files = Vec::new();

    // Find potential grid files
    match fs::read_dir(&base_path) {
        Ok(entries) => {
            for entry_result in entries {
                if let Ok(entry) = entry_result {
                    let path = entry.path();
                    if path.is_file() {
                        let is_json = path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("json"));
                        let is_meta_file = path.file_name().map_or(false, |name| name.to_string_lossy().ends_with(".meta.json"));
                        if is_json && !is_meta_file { potential_grid_files.push(path); }
                    }
                } else if let Err(e) = entry_result { error!("Startup Scan: Failed processing entry in '{:?}': {}", base_path, e); }
            }
        }
        Err(e) => { error!("Startup Scan: Failed to read directory '{:?}': {}", base_path, e); return; }
    }

    if potential_grid_files.is_empty() { info!("Startup Scan: No potential unregistered grid files (.json) found."); return; }
    else { trace!("Startup Scan: Found potential grid files: {:?}", potential_grid_files); }


    // Process potential files
    for path in potential_grid_files {
        let filename = path.file_name().map_or_else(|| "unknown.json".to_string(), |os| os.to_string_lossy().into_owned());
        let sheet_name_candidate = path.file_stem().map_or_else(
            || filename.trim_end_matches(".json").trim_end_matches(".JSON").to_string(),
            |os| os.to_string_lossy().into_owned()
        );

        if sheet_name_candidate.is_empty() || sheet_name_candidate.starts_with('.') {
            warn!("Startup Scan: Skipping file '{}': Invalid sheet name derived ('{}').", filename, sheet_name_candidate);
            continue;
        }

        let already_registered = registry.get_sheet(&sheet_name_candidate).is_some() ||
            registry.iter_sheets().any(|(_, data)| data.metadata.as_ref().map_or(false, |m| m.data_filename == filename));

        if !already_registered {
            trace!("Startup Scan: Found potential unregistered grid file: '{}'. Attempting load as sheet '{}'.", filename, sheet_name_candidate);
            let mut loaded_metadata: Option<SheetMetadata> = None;
            let meta_filename = format!("{}.meta.json", sheet_name_candidate);
            let meta_path = base_path.join(&meta_filename);
            let mut needs_save_after_correction = false;

            // Try loading corresponding metadata file
            if meta_path.exists() {
                match read_and_parse_metadata_file(&meta_path) {
                    Ok(mut meta) => {
                        if meta.sheet_name != sheet_name_candidate {
                            warn!("Startup Scan: Correcting sheet_name in '{}' to match candidate '{}'.", meta_filename, sheet_name_candidate);
                            meta.sheet_name = sheet_name_candidate.clone();
                            needs_save_after_correction = true;
                        }
                        if meta.data_filename != filename {
                            warn!("Startup Scan: Correcting data_filename in '{}' to match grid file ('{}').", meta_filename, filename);
                            meta.data_filename = filename.clone();
                            needs_save_after_correction = true;
                        }

                        // Ensure validator consistency for loaded metadata
                        if meta.ensure_validator_consistency() {
                             info!("Startup Scan: Corrected validator/type/filter consistency for loaded metadata of '{}'.", sheet_name_candidate);
                             needs_save_after_correction = true;
                        }

                        trace!("Startup Scan: Loaded metadata for '{}' from '{}'.", sheet_name_candidate, meta_filename);
                        loaded_metadata = Some(meta);
                    }
                    Err(e) => { error!("Startup Scan: Failed to load metadata file '{}' for sheet '{}': {}", meta_filename, sheet_name_candidate, e); }
                }
            } else { trace!("Startup Scan: Metadata file '{}' not found.", meta_filename); }


            // Try loading grid data
            match read_and_parse_json_sheet(&path) {
                Ok((grid_data, _)) => {
                    let final_metadata = loaded_metadata.unwrap_or_else(|| {
                        info!("Startup Scan: Generating default metadata for sheet '{}'.", sheet_name_candidate);
                         needs_save_after_correction = true; // Generated needs save
                        let num_cols = grid_data.first().map_or(0, |r| r.len());
                        SheetMetadata::create_generic(sheet_name_candidate.clone(), filename.clone(), num_cols)
                    });
                    // Consistency checks already done on loaded/generated meta

                    let sheet_data = SheetGridData { metadata: Some(final_metadata), grid: grid_data };
                    registry.add_or_replace_sheet(sheet_name_candidate.clone(), sheet_data);
                    found_unregistered_count += 1;
                    info!("Startup Scan: Loaded and registered sheet '{}' from file '{}'.", sheet_name_candidate, filename);

                    if needs_save_after_correction { save_single_sheet(&registry, &sheet_name_candidate); }
                }
                Err(e) => {
                     if e.to_lowercase().contains("file is empty") {
                          info!("Startup Scan: Grid file '{}' for '{}' is empty. Registering with empty grid.", filename, sheet_name_candidate);
                          let final_metadata = loaded_metadata.unwrap_or_else(|| {
                               needs_save_after_correction = true;
                              SheetMetadata::create_generic(sheet_name_candidate.clone(), filename.clone(), 0)
                          });
                          // Consistency checks done

                          let sheet_data = SheetGridData { metadata: Some(final_metadata), grid: Vec::new() };
                          registry.add_or_replace_sheet(sheet_name_candidate.clone(), sheet_data);
                          found_unregistered_count += 1;
                          info!("Startup Scan: Registered empty sheet '{}'.", sheet_name_candidate);
                          if needs_save_after_correction { save_single_sheet(&registry, &sheet_name_candidate); }
                     } else {
                          error!("Startup Scan: Failed to load grid data from '{}' for sheet '{}': {}", filename, sheet_name_candidate, e);
                     }
                }
            }
        } else {
            trace!("Startup Scan: Skipping file '{}': Sheet '{}' already registered or filename conflict.", filename, sheet_name_candidate);
        }
    } // End processing loop

    if found_unregistered_count > 0 { info!("Startup Scan: Found and processed {} manually added sheets.", found_unregistered_count); }
    else { info!("Startup Scan: No new manually added sheets found to process."); }
}