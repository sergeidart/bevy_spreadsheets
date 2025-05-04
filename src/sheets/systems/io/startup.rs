// src/sheets/systems/io/startup.rs
use bevy::prelude::*;
use std::{fs, path::{Path, PathBuf}}; // Combine path imports
use super::get_default_data_base_path;
use super::save::save_single_sheet;
use super::parsers::{read_and_parse_json_sheet, read_and_parse_metadata_file};
use super::validator::{self, validate_file_exists}; // Can use validator::* or specific functions
use crate::sheets::{definitions::{SheetGridData, SheetMetadata},resources::SheetRegistry};
use crate::example_definitions::{create_example_items_metadata, create_simple_config_metadata};

pub fn register_sheet_metadata(mut registry: ResMut<SheetRegistry>) {
    let data_dir_path = get_default_data_base_path();
    if !data_dir_path.exists() {
        info!("Data directory '{:?}' does not exist. Registering default template sheets.", data_dir_path);
        if let Err(e) = fs::create_dir_all(&data_dir_path) {
             error!("Failed to create data directory {:?}: {}. Cannot register or save template sheets.", data_dir_path, e);
             return;
        }
        let registered_example = registry.register(create_example_items_metadata());
        let registered_config = registry.register(create_simple_config_metadata());
        if registered_example || registered_config {
             info!("Registered pre-defined template sheet metadata.");
             if registered_example { save_single_sheet(&registry, "ExampleItems"); }
             if registered_config { save_single_sheet(&registry, "SimpleConfig"); }
        }
    } else {
         info!("Data directory '{:?}' already exists. Skipping registration of default template sheets.", data_dir_path);
    }
}

pub fn load_registered_sheets_startup(mut registry: ResMut<SheetRegistry>) {
    info!("Startup: Loading data for registered sheets...");
    let base_path = get_default_data_base_path();

    if !base_path.exists() {
        info!("Startup: Data directory '{:?}' does not exist yet. Skipping load.", base_path);
        return;
    }

    let sheet_names_to_process: Vec<String> = registry.get_sheet_names().clone();
    if sheet_names_to_process.is_empty() {
        info!("Startup: No pre-registered sheets found to load.");
        return;
    }
    for sheet_name in sheet_names_to_process {
        load_and_update_single_registered_sheet(&sheet_name, &mut registry, &base_path);
    }

    info!("Startup: Finished loading data for registered sheets.");
}

fn load_and_update_single_registered_sheet(
    sheet_name: &str,
    registry: &mut SheetRegistry,
    base_path: &Path,
) {
    trace!("Startup: Processing registered sheet '{}'", sheet_name);
    let mut loaded_and_validated_metadata: Option<SheetMetadata> = None;
    let mut final_grid_filename: Option<String> = None;
    let meta_filename = format!("{}.meta.json", sheet_name);
    let meta_path = base_path.join(&meta_filename);

    if validate_file_exists(&meta_path).is_ok() {
        match read_and_parse_metadata_file(&meta_path) {
            Ok(mut loaded_meta) => {
                let expected_grid_filename = registry.get_sheet(sheet_name)
                    .and_then(|s| s.metadata.as_ref())
                    .map(|m| m.data_filename.clone())
                    .unwrap_or_else(|| format!("{}.json", sheet_name));

                match validator::validate_or_correct_loaded_metadata(&mut loaded_meta, sheet_name, &expected_grid_filename, true) {
                    Ok(()) => {
                         trace!("Startup: Loaded/validated metadata for '{}'.", sheet_name);
                         final_grid_filename = Some(loaded_meta.data_filename.clone());
                         loaded_and_validated_metadata = Some(loaded_meta);
                    }
                    Err(e) => { // Should only happen if warnings_only=false
                        error!("Startup: Uncorrectable errors in metadata for '{}': {}. Skipping.", sheet_name, e);
                        final_grid_filename = registry.get_sheet(sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
                    }
                }
            }
            Err(e) => {
                error!("Startup: Failed to read/parse metadata for '{}': {}.", sheet_name, e);
                 final_grid_filename = registry.get_sheet(sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
            }
        }
    } else {
        trace!("Startup: Metadata file not found for '{}'.", sheet_name);
         final_grid_filename = registry.get_sheet(sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
    }

    let mut loaded_grid_data: Option<Vec<Vec<String>>> = None;
    if let Some(grid_filename) = &final_grid_filename {
        if grid_filename.is_empty() {
            warn!("Startup: Skipping grid load for '{}': Filename is empty.", sheet_name);
        } else {
            let full_grid_path = base_path.join(grid_filename);
            if validate_file_exists(&full_grid_path).is_ok() {
                 match read_and_parse_json_sheet(&full_grid_path) {
                      Ok((grid_data, _)) => { loaded_grid_data = Some(grid_data); }
                      Err(e) => { error!("Startup: Failed to read/parse grid for '{}': {}", sheet_name, e); }
                 }
            } else {
                 trace!("Startup: Grid data file not found for '{}'.", sheet_name);
            }
        }
    } else {
         warn!("Startup: Cannot load grid for '{}': No filename identified.", sheet_name);
    }

    if let Some(sheet_entry) = registry.get_sheet_mut(sheet_name) {
         let mut grid_validation_passed = false;
         if let Some(meta) = loaded_and_validated_metadata {
             sheet_entry.metadata = Some(meta);
         } else if sheet_entry.metadata.is_none() {
              warn!("Startup: Generating default metadata for '{}'.", sheet_name);
              let default_filename = final_grid_filename.clone().unwrap_or_else(|| format!("{}.json", sheet_name));
              let num_cols = loaded_grid_data.as_ref().map_or(0, |g| g.first().map_or(0, |r| r.len()));
              sheet_entry.metadata = Some(SheetMetadata::create_generic(sheet_name.to_string(), default_filename, num_cols));
         }
          if let (Some(grid), Some(meta)) = (&loaded_grid_data, sheet_entry.metadata.as_ref()) {
               match validator::validate_grid_structure(grid, meta, sheet_name) {
                    Ok(()) => { grid_validation_passed = true; }
                    Err(e) => {
                         warn!("Startup: Grid validation failed for '{}': {}. Allowing load with warning.", sheet_name, e);
                         grid_validation_passed = true; // Or set to false
                    }
               }
          } else if loaded_grid_data.is_some() {
               warn!("Startup: Cannot validate grid structure for '{}': Metadata unavailable.", sheet_name);
               grid_validation_passed = true; // Allow load?
          }
          if grid_validation_passed {
               if let Some(grid) = loaded_grid_data {
                    sheet_entry.grid = grid;
                    trace!("Startup: Successfully loaded grid data for '{}'.", sheet_name);
               }
          } else if loaded_grid_data.is_some() {
               warn!("Startup: Skipping grid update for '{}' due to validation failure.", sheet_name);
               // sheet_entry.grid.clear(); // Optional: Clear if strict
          }
    } else {
         error!("Startup: Sheet '{}' disappeared from registry during processing!", sheet_name);
    }
}

pub fn scan_directory_for_sheets_startup(mut registry: ResMut<SheetRegistry>) {
    let base_path = get_default_data_base_path();
    info!("Startup: Scanning directory '{:?}' for manually added sheets...", base_path);

    if !base_path.exists() {
        info!("Startup: Data directory does not exist. No manual sheets to scan.");
        return;
    }

    let mut found_unregistered_count = 0;
    let mut potential_grid_files = Vec::new();
    match fs::read_dir(&base_path) {
        Ok(entries) => {
            for entry_result in entries {
                 if let Ok(entry) = entry_result {
                     let path = entry.path();
                     if path.is_file() {
                         let is_json = path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("json"));
                         let is_meta_file = path.file_name().map_or(false, |name| name.to_string_lossy().ends_with(".meta.json"));
                         if is_json && !is_meta_file {
                              potential_grid_files.push(path);
                         }
                     }
                 } else if let Err(e) = entry_result {
                      error!("Startup: Failed processing entry in '{:?}': {}", base_path, e);
                 }
            }
        }
        Err(e) => {
            error!("Startup: Failed to read directory '{:?}': {}", base_path, e);
            return;
         }
    }

    for path in potential_grid_files {
          let filename = path.file_name().map_or_else(|| "unknown.json".to_string(), |os| os.to_string_lossy().into_owned());
          let sheet_name_candidate = path.file_stem().map_or_else(|| filename.trim_end_matches(".json").trim_end_matches(".JSON").to_string(), |os| os.to_string_lossy().into_owned());

          if sheet_name_candidate.is_empty() {
               warn!("Startup Scan: Skipping file '{}': Empty sheet name derived.", filename);
               continue;
          }

          let already_registered = registry.get_sheet(&sheet_name_candidate).is_some() ||
              registry.iter_sheets().any(|(_, data)| data.metadata.as_ref().map_or(false, |m| m.data_filename == filename));

          if !already_registered {
                trace!("Startup Scan: Found potential unregistered grid file: '{}'. Attempting load as '{}'.", filename, sheet_name_candidate);
                let mut loaded_metadata: Option<SheetMetadata> = None;
                let meta_filename = format!("{}.meta.json", sheet_name_candidate);
                let meta_path = base_path.join(&meta_filename);
                if meta_path.exists() {
                     match read_and_parse_metadata_file(&meta_path) {
                         Ok(mut meta) => {
                              if meta.sheet_name != sheet_name_candidate {
                                  warn!("Startup Scan: Correcting sheet_name in '{}' ('{}') to match candidate '{}'.", meta_filename, meta.sheet_name, sheet_name_candidate);
                                  meta.sheet_name = sheet_name_candidate.clone();
                              }
                              if meta.data_filename != filename {
                                   warn!("Startup Scan: Correcting data_filename in '{}' ('{}') to match grid file ('{}').", meta_filename, meta.data_filename, filename);
                                   meta.data_filename = filename.clone();
                              }
                              trace!("Startup Scan: Loaded metadata for '{}'.", sheet_name_candidate);
                              loaded_metadata = Some(meta);
                         }
                         Err(e) => { error!("Startup Scan: Failed to load metadata '{}' for '{}': {}", meta_filename, sheet_name_candidate, e); }
                     }
                } else { trace!("Startup Scan: Metadata file '{}' not found.", meta_filename); }
                 match read_and_parse_json_sheet(&path) {
                     Ok((grid_data, _)) => {
                         let final_metadata = loaded_metadata.unwrap_or_else(|| {
                              info!("Startup Scan: Generating default metadata for sheet '{}'.", sheet_name_candidate);
                              let num_cols = grid_data.first().map_or(0, |r| r.len());
                              SheetMetadata::create_generic(sheet_name_candidate.clone(), filename.clone(), num_cols)
                         });
                         let sheet_data = SheetGridData { metadata: Some(final_metadata), grid: grid_data };
                         registry.add_or_replace_sheet(sheet_name_candidate.clone(), sheet_data);
                         found_unregistered_count += 1;
                         info!("Startup Scan: Loaded and registered sheet '{}'.", sheet_name_candidate);
                         save_single_sheet(&registry, &sheet_name_candidate);
                     }
                     Err(e) => {
                         if !e.contains("File is empty") {
                              error!("Startup Scan: Failed to load grid '{}' for '{}': {}", filename, sheet_name_candidate, e);
                         } else {
                              info!("Startup Scan: Grid file '{}' for '{}' is empty. Registering.", filename, sheet_name_candidate);
                               let final_metadata = loaded_metadata.unwrap_or_else(|| SheetMetadata::create_generic(sheet_name_candidate.clone(), filename.clone(), 0));
                               let sheet_data = SheetGridData { metadata: Some(final_metadata), grid: Vec::new() };
                               registry.add_or_replace_sheet(sheet_name_candidate.clone(), sheet_data);
                               found_unregistered_count += 1;
                               info!("Startup Scan: Registered empty sheet '{}'.", sheet_name_candidate);
                               save_single_sheet(&registry, &sheet_name_candidate); // Save empty sheet + meta
                         }
                     }
                 } // End grid load match
            } else { trace!("Startup Scan: Skipping '{}': Already registered.", filename); }
       } // End processing loop
    if found_unregistered_count > 0 {
         info!("Startup Scan: Found and processed {} manually added sheets.", found_unregistered_count);
    } else {
         info!("Startup Scan: No new manually added sheets found.");
    }
}