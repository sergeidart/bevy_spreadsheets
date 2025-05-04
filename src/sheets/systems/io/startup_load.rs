// src/sheets/systems/io/startup_load.rs
use bevy::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::get_default_data_base_path;
use super::save::save_single_sheet;
use super::parsers::{read_and_parse_json_sheet, read_and_parse_metadata_file};
use super::validator::{self, validate_file_exists};
use crate::sheets::{
    definitions::{SheetMetadata, SheetGridData}, // Removed unused ColumnDataType import
    resources::SheetRegistry,
};
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
            if registered_example { info!("Saving template sheet: ExampleItems"); save_single_sheet(&registry, "ExampleItems"); }
            if registered_config { info!("Saving template sheet: SimpleConfig"); save_single_sheet(&registry, "SimpleConfig"); }
        }
    } else {
        info!("Data directory '{:?}' already exists. Skipping registration of default template sheets.", data_dir_path);
    }
}

pub fn load_registered_sheets_startup(mut registry: ResMut<SheetRegistry>) {
    info!("Startup Load: Loading data for registered sheets...");
    let base_path = get_default_data_base_path();

    if !base_path.exists() {
        info!("Startup Load: Data directory '{:?}' does not exist yet. Skipping load.", base_path);
        return;
    }

    let sheet_names_to_process: Vec<String> = registry.get_sheet_names().clone();
    if sheet_names_to_process.is_empty() {
        info!("Startup Load: No pre-registered sheets found to load.");
        return;
    }

    let mut sheets_corrected_and_need_save = Vec::new();

    for sheet_name in &sheet_names_to_process {
        if load_and_update_single_registered_sheet(sheet_name, &mut registry, &base_path) {
             sheets_corrected_and_need_save.push(sheet_name.clone());
        }
    }

    if !sheets_corrected_and_need_save.is_empty() {
        info!("Startup Load: Saving sheets that required metadata correction: {:?}", sheets_corrected_and_need_save);
        let registry_immut = registry.as_ref();
        for sheet_name in sheets_corrected_and_need_save {
             save_single_sheet(registry_immut, &sheet_name);
        }
    }

    info!("Startup Load: Finished loading data for registered sheets.");
}

fn load_and_update_single_registered_sheet(
    sheet_name: &str,
    registry: &mut SheetRegistry,
    base_path: &Path,
) -> bool {
    trace!("Startup Load: Processing registered sheet '{}'", sheet_name);
    let mut loaded_and_validated_metadata: Option<SheetMetadata> = None;
    let mut final_grid_filename: Option<String> = None;
    let meta_filename = format!("{}.meta.json", sheet_name);
    let meta_path = base_path.join(&meta_filename);
    let mut needs_save_after_correction = false;

    // --- Load and Validate Metadata ---
    if validate_file_exists(&meta_path).is_ok() {
        match read_and_parse_metadata_file(&meta_path) {
            Ok(mut loaded_meta) => {
                let expected_grid_filename = registry.get_sheet(sheet_name)
                    .and_then(|s| s.metadata.as_ref())
                    .map(|m| m.data_filename.clone())
                    .unwrap_or_else(|| format!("{}.json", sheet_name));

                match validator::validate_or_correct_loaded_metadata(&mut loaded_meta, sheet_name, &expected_grid_filename, true) {
                    Ok(()) => {
                        trace!("Startup Load: Basic metadata validation/correction passed for '{}'.", sheet_name);
                        final_grid_filename = Some(loaded_meta.data_filename.clone());

                        // Ensure validator consistency AFTER basic validation
                        if loaded_meta.ensure_validator_consistency() {
                             info!("Startup Load: Corrected validator/type/filter consistency for loaded metadata of sheet '{}'.", sheet_name);
                             needs_save_after_correction = true;
                        }

                        loaded_and_validated_metadata = Some(loaded_meta);
                    }
                    Err(e) => {
                        error!("Startup Load: Uncorrectable errors during basic metadata validation for '{}': {}. Skipping metadata load.", sheet_name, e);
                        final_grid_filename = registry.get_sheet(sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
                    }
                }
            }
            Err(e) => {
                error!("Startup Load: Failed to read/parse metadata file for '{}': {}.", sheet_name, e);
                final_grid_filename = registry.get_sheet(sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
            }
        }
    } else {
        trace!("Startup Load: Metadata file not found for '{}'.", sheet_name);
        final_grid_filename = registry.get_sheet(sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
    }

    // --- Load Grid Data ---
    let mut loaded_grid_data: Option<Vec<Vec<String>>> = None;
    if let Some(grid_filename) = &final_grid_filename {
        if grid_filename.is_empty() {
            warn!("Startup Load: Skipping grid load for '{}': Filename is empty.", sheet_name);
        } else {
            let full_grid_path = base_path.join(grid_filename);
            if validate_file_exists(&full_grid_path).is_ok() {
                 match read_and_parse_json_sheet(&full_grid_path) {
                      Ok((grid_data, _)) => { loaded_grid_data = Some(grid_data); }
                      Err(e) => { error!("Startup Load: Failed to read/parse grid file '{}' for sheet '{}': {}", grid_filename, sheet_name, e); }
                 }
            } else { trace!("Startup Load: Grid data file '{}' not found for '{}'.", grid_filename, sheet_name); }
        }
    } else { warn!("Startup Load: Cannot load grid for '{}': No data filename identified.", sheet_name); }

    // --- Update Registry Entry ---
    if let Some(sheet_entry) = registry.get_sheet_mut(sheet_name) {
         let mut grid_validation_passed = false;

         // Update metadata in registry if loaded successfully
         if let Some(loaded_meta) = loaded_and_validated_metadata {
             sheet_entry.metadata = Some(loaded_meta); // Already includes consistency checks
         } else if sheet_entry.metadata.is_none() {
             // Generate default metadata if none exists and none was loaded
             warn!("Startup Load: Generating default metadata for '{}'.", sheet_name);
             let default_filename = final_grid_filename.clone().unwrap_or_else(|| format!("{}.json", sheet_name));
             let num_cols = loaded_grid_data.as_ref().and_then(|g| g.first()).map_or(0, |r| r.len());
             sheet_entry.metadata = Some(SheetMetadata::create_generic(sheet_name.to_string(), default_filename, num_cols));
             needs_save_after_correction = true;
         } else {
             // Ensure consistency of existing registry metadata if file wasn't loaded/validated
             if let Some(meta) = &mut sheet_entry.metadata {
                 if meta.ensure_validator_consistency() {
                     info!("Startup Load: Corrected validator/type/filter consistency for existing registry metadata for '{}'.", sheet_name);
                     needs_save_after_correction = true;
                 }
             }
         }

         // Validate grid structure against final metadata
         if let (Some(grid), Some(meta)) = (&loaded_grid_data, sheet_entry.metadata.as_ref()) {
              match validator::validate_grid_structure(grid, meta, sheet_name) {
                   Ok(()) => { grid_validation_passed = true; }
                   Err(e) => { warn!("Startup Load: Grid structure validation failed for '{}': {}. Allowing load.", sheet_name, e); grid_validation_passed = true; }
              }
         } else if loaded_grid_data.is_some() {
              warn!("Startup Load: Cannot validate grid structure for '{}': Metadata unavailable.", sheet_name);
              grid_validation_passed = true;
         } else { grid_validation_passed = true; }


         // Update grid data if loaded and validation passed
         if grid_validation_passed {
              if let Some(grid) = loaded_grid_data {
                   sheet_entry.grid = grid;
                   trace!("Startup Load: Successfully loaded grid data for '{}'.", sheet_name);
              }
         } else if loaded_grid_data.is_some() {
              warn!("Startup Load: Skipping grid update for '{}' due to structure validation failure.", sheet_name);
         }
    }

    needs_save_after_correction
}