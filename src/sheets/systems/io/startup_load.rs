// src/sheets/systems/io/startup_load.rs
use bevy::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

// Corrected imports
use super::{get_default_data_base_path, get_full_metadata_path, get_full_sheet_path};
use super::save::save_single_sheet;
use super::parsers::{read_and_parse_json_sheet, read_and_parse_metadata_file};
use super::validator::{self, validate_file_exists};
use crate::sheets::{
    definitions::{SheetMetadata, SheetGridData},
    resources::SheetRegistry,
};
use crate::example_definitions::{create_example_items_metadata, create_simple_config_metadata};

/// Registers default sheets if the data directory is empty.
pub fn register_sheet_metadata(mut registry: ResMut<SheetRegistry>) {
    let data_dir_path = get_default_data_base_path();
    if !data_dir_path.exists() || fs::read_dir(&data_dir_path).map_or(true, |mut dir| dir.next().is_none()) {
        info!("Data directory '{:?}' is missing or empty. Registering default template sheets.", data_dir_path);
        if !data_dir_path.exists() {
            if let Err(e) = fs::create_dir_all(&data_dir_path) {
                error!("Failed to create data directory {:?}: {}. Cannot register or save template sheets.", data_dir_path, e);
                return;
            }
        }

        // Register example sheets (they have category=None implicitly or explicitly)
        let example_meta = create_example_items_metadata(); // Category should be None
        let config_meta = create_simple_config_metadata();   // Category should be None

        let registered_example = registry.register(example_meta.clone());
        let registered_config = registry.register(config_meta.clone());

        if registered_example || registered_config {
            info!("Registered pre-defined template sheet metadata.");
            // Need immutable borrow for saving
            let registry_immut = registry.as_ref();
            if registered_example {
                 info!("Saving template sheet: {}", example_meta.sheet_name);
                 save_single_sheet(registry_immut, &example_meta); // Pass metadata
            }
            if registered_config {
                info!("Saving template sheet: {}", config_meta.sheet_name);
                save_single_sheet(registry_immut, &config_meta); // Pass metadata
            }
        }
    } else {
        info!("Data directory '{:?}' already exists and is not empty. Skipping registration of default template sheets.", data_dir_path);
    }
}

/// Loads grid data for sheets already registered (e.g., from `register_sheet_metadata`).
pub fn load_registered_sheets_startup(mut registry: ResMut<SheetRegistry>) {
    info!("Startup Load: Loading data for registered sheets...");
    let base_path = get_default_data_base_path();

    if !base_path.exists() {
        info!("Startup Load: Data directory '{:?}' does not exist yet. Skipping load.", base_path);
        return;
    }

    // Get all registered sheet identifiers (category, name)
    let sheet_identifiers: Vec<(Option<String>, String)> = registry
        .iter_sheets()
        .map(|(cat, name, _)| (cat.clone(), name.clone()))
        .collect();

    if sheet_identifiers.is_empty() {
        info!("Startup Load: No pre-registered sheets found to load.");
        return;
    } else {
         trace!("Startup Load: Found registered sheets: {:?}", sheet_identifiers);
    }

    let mut sheets_corrected_and_need_save = Vec::new(); // Store metadata that needs saving

    for (category, sheet_name) in &sheet_identifiers {
        if load_and_update_single_registered_sheet(
            category,
            sheet_name,
            &mut registry,
            &base_path,
        ) {
             // Retrieve the metadata again to store it for saving
             if let Some(data) = registry.get_sheet(category, sheet_name) {
                 if let Some(meta) = &data.metadata {
                      sheets_corrected_and_need_save.push(meta.clone());
                 }
             }
        }
    }

    // Save sheets that required corrections
    if !sheets_corrected_and_need_save.is_empty() {
        info!("Startup Load: Saving sheets that required metadata correction: {:?}", sheets_corrected_and_need_save.iter().map(|m|m.sheet_name.as_str()).collect::<Vec<_>>());
        let registry_immut = registry.as_ref();
        for metadata_to_save in sheets_corrected_and_need_save {
            save_single_sheet(registry_immut, &metadata_to_save); // Pass metadata
        }
    }

    info!("Startup Load: Finished loading data for registered sheets.");
}

/// Loads and validates data for a single registered sheet, updating the registry entry.
/// Returns true if metadata was corrected and needs saving.
fn load_and_update_single_registered_sheet(
    category: &Option<String>,
    sheet_name: &str,
    registry: &mut SheetRegistry,
    base_path: &Path,
) -> bool {
    trace!("Startup Load: Processing registered sheet '{:?}/{}'", category, sheet_name);
    let mut loaded_and_validated_metadata: Option<SheetMetadata> = None;
    let mut final_grid_filename_only: Option<String> = None; // Just the filename part
    let mut needs_save_after_correction = false;

    // --- Determine Expected Paths using Metadata from Registry ---
    let (expected_meta_path, expected_grid_path, expected_grid_filename_only) = {
        if let Some(sheet_data) = registry.get_sheet(category, sheet_name) {
            if let Some(meta) = &sheet_data.metadata {
                 // Derive paths using helpers
                 let meta_p = get_full_metadata_path(base_path, meta);
                 let grid_p = get_full_sheet_path(base_path, meta);
                 (Some(meta_p), Some(grid_p), Some(meta.data_filename.clone()))
            } else {
                 warn!("Startup Load: Metadata missing in registry for '{:?}/{}', cannot determine file paths accurately.", category, sheet_name);
                 (None, None, None)
            }
        } else {
             error!("Startup Load: Sheet '{:?}/{}' disappeared from registry during processing.", category, sheet_name);
             (None, None, None)
        }
    };

    // Store the expected grid filename for later use
    final_grid_filename_only = expected_grid_filename_only;


    // --- Load and Validate Metadata ---
    if let Some(meta_path) = expected_meta_path {
         if validate_file_exists(&meta_path).is_ok() {
             match read_and_parse_metadata_file(&meta_path) {
                 Ok(mut loaded_meta) => {
                     // Extract expected filename from registry meta again
                     let expected_grid_filename_reg = registry.get_sheet(category, sheet_name)
                        .and_then(|s| s.metadata.as_ref())
                        .map(|m| m.data_filename.clone())
                        .unwrap_or_else(|| format!("{}.json", sheet_name)); // Fallback

                     // --- Basic Validation/Correction ---
                     match validator::validate_or_correct_loaded_metadata(
                         &mut loaded_meta,
                         sheet_name, // Expected name
                         &expected_grid_filename_reg, // Expected filename from registry
                         true, // Warnings only, correct in place
                     ) {
                         Ok(()) => {
                             trace!("Startup Load: Basic metadata validation/correction passed for '{:?}/{}'.", category, sheet_name);
                             // Ensure loaded metadata category matches the registry category key
                             if loaded_meta.category != *category {
                                 warn!("Startup Load: Correcting category in loaded metadata for '{}' from '{:?}' to '{:?}' based on registry.", sheet_name, loaded_meta.category, category);
                                 loaded_meta.category = category.clone();
                                 needs_save_after_correction = true;
                             }

                             // Store the (potentially corrected) filename from loaded meta
                             final_grid_filename_only = Some(loaded_meta.data_filename.clone());

                             // Ensure validator consistency AFTER basic validation
                             if loaded_meta.ensure_validator_consistency() {
                                 info!("Startup Load: Corrected validator/type/filter consistency for loaded metadata of sheet '{:?}/{}'.", category, sheet_name);
                                 needs_save_after_correction = true;
                             }
                             loaded_and_validated_metadata = Some(loaded_meta);
                         }
                         Err(e) => {
                             error!("Startup Load: Uncorrectable errors during basic metadata validation for '{:?}/{}': {}. Skipping metadata load.", category, sheet_name, e);
                             // Keep filename from registry meta if validation failed
                             final_grid_filename_only = registry.get_sheet(category, sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
                         }
                     }
                 }
                 Err(e) => {
                     error!("Startup Load: Failed to read/parse metadata file for '{:?}/{}': {}.", category, sheet_name, e);
                     // Keep filename from registry meta if parse failed
                     final_grid_filename_only = registry.get_sheet(category, sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
                 }
             }
         } else {
             trace!("Startup Load: Metadata file not found for '{:?}/{}'. Path: {}", category, sheet_name, meta_path.display());
             // Keep filename from registry meta if file not found
             final_grid_filename_only = registry.get_sheet(category, sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
         }
    } else {
         warn!("Startup Load: Could not determine metadata path for '{:?}/{}'.", category, sheet_name);
         // Keep filename from registry meta if path unknown
         final_grid_filename_only = registry.get_sheet(category, sheet_name).and_then(|s| s.metadata.as_ref()).map(|m| m.data_filename.clone());
    }


    // --- Load Grid Data ---
    let mut loaded_grid_data: Option<Vec<Vec<String>>> = None;
    if let Some(grid_filename) = &final_grid_filename_only {
        if grid_filename.is_empty() {
            warn!("Startup Load: Skipping grid load for '{:?}/{}': Filename is empty.", category, sheet_name);
        } else {
            // Construct the full grid path using the determined filename and category
             let mut full_grid_path = base_path.to_path_buf();
             if let Some(cat_name) = category {
                 full_grid_path.push(cat_name);
             }
             full_grid_path.push(grid_filename);


            if validate_file_exists(&full_grid_path).is_ok() {
                match read_and_parse_json_sheet(&full_grid_path) {
                    Ok((grid_data, _)) => { loaded_grid_data = Some(grid_data); }
                    Err(e) => { error!("Startup Load: Failed to read/parse grid file '{}' for sheet '{:?}/{}': {}", full_grid_path.display(), category, sheet_name, e); }
                }
            } else {
                trace!("Startup Load: Grid data file '{}' not found for '{:?}/{}'.", full_grid_path.display(), category, sheet_name);
            }
        }
    } else {
        warn!("Startup Load: Cannot load grid for '{:?}/{}': No data filename identified.", category, sheet_name);
    }

    // --- Update Registry Entry ---
    if let Some(sheet_entry) = registry.get_sheet_mut(category, sheet_name) {
        let mut grid_validation_passed = false;

        // Update metadata in registry if loaded successfully
        if let Some(loaded_meta) = loaded_and_validated_metadata {
            sheet_entry.metadata = Some(loaded_meta); // Already includes consistency checks
        } else if sheet_entry.metadata.is_none() {
            // Generate default metadata if none exists and none was loaded
            warn!("Startup Load: Generating default metadata for '{:?}/{}'.", category, sheet_name);
            let default_filename = final_grid_filename_only.clone().unwrap_or_else(|| format!("{}.json", sheet_name));
            let num_cols = loaded_grid_data.as_ref().and_then(|g| g.first()).map_or(0, |r| r.len());
            sheet_entry.metadata = Some(SheetMetadata::create_generic(
                sheet_name.to_string(),
                default_filename,
                num_cols,
                category.clone() // Add category
            ));
            needs_save_after_correction = true;
        } else {
            // Ensure consistency of existing registry metadata if file wasn't loaded/validated
            if let Some(meta) = &mut sheet_entry.metadata {
                if meta.ensure_validator_consistency() {
                    info!("Startup Load: Corrected validator/type/filter consistency for existing registry metadata for '{:?}/{}'.", category, sheet_name);
                    needs_save_after_correction = true;
                }
                // Also ensure category is correct in existing metadata
                if meta.category != *category {
                    warn!("Startup Load: Correcting category in existing registry metadata for '{}' from '{:?}' to '{:?}'.", sheet_name, meta.category, category);
                    meta.category = category.clone();
                    needs_save_after_correction = true;
                }
            }
        }

        // Validate grid structure against final metadata
        if let (Some(grid), Some(meta)) = (&loaded_grid_data, sheet_entry.metadata.as_ref()) {
            match validator::validate_grid_structure(grid, meta, sheet_name) {
                Ok(()) => { grid_validation_passed = true; }
                Err(e) => { warn!("Startup Load: Grid structure validation failed for '{:?}/{}': {}. Allowing load.", category, sheet_name, e); grid_validation_passed = true; } // Still load grid even if invalid? Or skip? Current allows.
            }
        } else if loaded_grid_data.is_some() {
            warn!("Startup Load: Cannot validate grid structure for '{:?}/{}': Metadata unavailable.", category, sheet_name);
            grid_validation_passed = true; // Allow load if metadata missing
        } else {
            // No grid data loaded, validation is implicitly passed (or N/A)
            grid_validation_passed = true;
        }

        // Update grid data if loaded and validation passed
        if grid_validation_passed {
            if let Some(grid) = loaded_grid_data {
                sheet_entry.grid = grid;
                trace!("Startup Load: Successfully loaded grid data for '{:?}/{}'.", category, sheet_name);
            }
        } else if loaded_grid_data.is_some() {
            warn!("Startup Load: Skipping grid update for '{:?}/{}' due to structure validation failure.", category, sheet_name);
        }
    }

    needs_save_after_correction
}