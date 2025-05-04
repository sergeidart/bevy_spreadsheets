// src/sheets/systems/io/startup_scan.rs
use bevy::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir; // <-- Add walkdir

// Corrected imports relative to this file's module position
use super::{get_default_data_base_path, get_full_metadata_path, get_full_sheet_path};
use super::save::save_single_sheet;
use super::parsers::{read_and_parse_json_sheet, read_and_parse_metadata_file};
use crate::sheets::{
    definitions::{SheetMetadata, SheetGridData},
    resources::SheetRegistry,
};

/// Scans the data directory recursively for sheets not yet in the registry.
pub fn scan_directory_for_sheets_startup(mut registry: ResMut<SheetRegistry>) {
    let base_path = get_default_data_base_path();
    info!("Startup Scan: Recursively scanning directory '{:?}' for sheets...", base_path);

    if !base_path.exists() {
        info!("Startup Scan: Data directory does not exist. Nothing to scan.");
        return;
    }

    let mut found_unregistered_count = 0;
    let mut potential_grid_files = Vec::new();

    // --- Use WalkDir for recursive scanning ---
    for entry_result in WalkDir::new(&base_path)
        .min_depth(1) // Skip the base_path itself
        .into_iter()
        .filter_map(Result::ok) // Ignore errors for now, log later if needed
        .filter(|e| e.file_type().is_file())
    {
        let path = entry_result.path();
        let is_json = path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("json"));
        let is_meta_file = path.file_name().map_or(false, |name| name.to_string_lossy().ends_with(".meta.json"));

        if is_json && !is_meta_file {
            potential_grid_files.push(path.to_path_buf());
        } else if is_meta_file {
             // We'll handle meta files when processing their corresponding grid file
             trace!("Startup Scan: Found meta file '{}', will process with grid file.", path.display());
        }
    }

    if potential_grid_files.is_empty() {
        info!("Startup Scan: No potential unregistered grid files (.json) found.");
        return;
    } else {
        trace!("Startup Scan: Found potential grid files: {:?}", potential_grid_files);
    }

    // --- Process potential files ---
    for grid_path in potential_grid_files {
        let filename = grid_path.file_name().map_or_else(
            || "unknown.json".to_string(),
            |os| os.to_string_lossy().into_owned(),
        );
        let sheet_name_candidate = grid_path.file_stem().map_or_else(
            || filename.trim_end_matches(".json").trim_end_matches(".JSON").to_string(),
            |os| os.to_string_lossy().into_owned(),
        );

        // --- Determine Category ---
        let relative_path = match grid_path.strip_prefix(&base_path) {
             Ok(rel) => rel,
             Err(_) => {
                 error!("Startup Scan: Failed to strip base path from '{}'. Skipping.", grid_path.display());
                 continue;
             }
        };
        let category: Option<String> = relative_path
            .parent() // Get the directory containing the file
            .and_then(|p| p.file_name()) // Get the dir name
            .map(|os_str| os_str.to_string_lossy().into_owned())
            .filter(|dir_name| !dir_name.is_empty()); // Ensure it's not the root

        trace!("Startup Scan: Processing '{}'. Derived Name: '{}'. Derived Category: '{:?}'", grid_path.display(), sheet_name_candidate, category);

        // --- Basic Validation ---
        if sheet_name_candidate.is_empty() || sheet_name_candidate.starts_with('.') {
            warn!("Startup Scan: Skipping file '{}': Invalid sheet name derived ('{}').", filename, sheet_name_candidate);
            continue;
        }
        if category.as_deref().map_or(false, |c| c.starts_with('.')) {
             warn!("Startup Scan: Skipping file '{}' in category '{:?}': Invalid category name.", filename, category);
             continue;
        }

        // --- Check if Already Registered (more complex now with categories) ---
        // We need to check if *any* sheet with this name exists, regardless of category,
        // because sheet names should ideally be unique globally for simplicity (e.g., linking).
        // OR, if a sheet in the *same* category *and* using the *same* filename exists.
        let already_registered_name = registry.does_sheet_exist(&sheet_name_candidate);
        let already_registered_file = registry
            .get_sheet(&category, &sheet_name_candidate) // Check specific slot first
            .map_or(false, |data| data.metadata.as_ref().map_or(false, |m| m.data_filename == filename));

        if already_registered_name && !already_registered_file {
             warn!(
                 "Startup Scan: Skipping file '{}' (Sheet name '{}' already exists, possibly in another category). Manual resolution required if this is intended as a separate sheet.",
                 grid_path.display(), sheet_name_candidate
             );
             continue;
        }
        if already_registered_file {
             trace!("Startup Scan: Skipping file '{}': Sheet '{}' in category '{:?}' seems already registered.", grid_path.display(), sheet_name_candidate, category);
             continue;
        }
        // If name doesn't exist OR if name exists but not in this category/file combo, proceed with load

        trace!("Startup Scan: Found potential unregistered grid file: '{}'. Attempting load as sheet '{}' in category '{:?}'.", filename, sheet_name_candidate, category);
        let mut loaded_metadata: Option<SheetMetadata> = None;
        let expected_meta_path = base_path.join(relative_path.with_file_name(format!("{}.meta.json", sheet_name_candidate)));
        let mut needs_save_after_correction = false;

        // --- Try loading corresponding metadata file ---
        if expected_meta_path.exists() {
            match read_and_parse_metadata_file(&expected_meta_path) {
                Ok(mut meta) => {
                    let mut metadata_corrected = false;
                    // Validate/Correct Name
                    if meta.sheet_name != sheet_name_candidate {
                        warn!("Startup Scan: Correcting sheet_name in '{}' from '{}' to match candidate '{}'.", expected_meta_path.display(), meta.sheet_name, sheet_name_candidate);
                        meta.sheet_name = sheet_name_candidate.clone();
                        metadata_corrected = true;
                    }
                    // Validate/Correct Category
                    if meta.category != category {
                         warn!("Startup Scan: Correcting category in '{}' from '{:?}' to match folder structure '{:?}'.", expected_meta_path.display(), meta.category, category);
                         meta.category = category.clone();
                         metadata_corrected = true;
                    }
                    // Validate/Correct Data Filename (ensure it's just the name, not path)
                    let filename_only = grid_path.file_name().map(|os| os.to_string_lossy().into_owned()).unwrap_or_default();
                    if meta.data_filename != filename_only {
                        warn!("Startup Scan: Correcting data_filename in '{}' from '{}' to match grid file ('{}').", expected_meta_path.display(), meta.data_filename, filename_only);
                        meta.data_filename = filename_only.clone(); // Use only filename
                        metadata_corrected = true;
                    }

                    // Ensure validator consistency
                    if meta.ensure_validator_consistency() {
                        info!("Startup Scan: Corrected validator/type/filter consistency for loaded metadata of '{}'.", sheet_name_candidate);
                        metadata_corrected = true;
                    }

                    if metadata_corrected { needs_save_after_correction = true; }
                    trace!("Startup Scan: Loaded metadata for '{}' from '{}'.", sheet_name_candidate, expected_meta_path.display());
                    loaded_metadata = Some(meta);
                }
                Err(e) => {
                    error!("Startup Scan: Failed to load metadata file '{}' for sheet '{}': {}. Will generate default metadata.", expected_meta_path.display(), sheet_name_candidate, e);
                }
            }
        } else {
            trace!("Startup Scan: Metadata file '{}' not found for '{}'.", expected_meta_path.display(), sheet_name_candidate);
        }

        // --- Try loading grid data ---
        match read_and_parse_json_sheet(&grid_path) {
            Ok((grid_data, _)) => { // We already have filename
                let final_metadata = loaded_metadata.unwrap_or_else(|| {
                    info!("Startup Scan: Generating default metadata for sheet '{}' category '{:?}'.", sheet_name_candidate, category);
                    needs_save_after_correction = true; // Generated needs save
                    let num_cols = grid_data.first().map_or(0, |r| r.len());
                    SheetMetadata::create_generic(
                        sheet_name_candidate.clone(),
                        filename.clone(), // Pass only filename
                        num_cols,
                        category.clone() // Pass category
                    )
                });
                // Consistency checks already done on loaded/generated meta

                let sheet_data = SheetGridData { metadata: Some(final_metadata), grid: grid_data };
                // Use the new registry method
                registry.add_or_replace_sheet(category.clone(), sheet_name_candidate.clone(), sheet_data);
                found_unregistered_count += 1;
                info!("Startup Scan: Loaded and registered sheet '{}' category '{:?}' from file '{}'.", sheet_name_candidate, category, grid_path.display());

                if needs_save_after_correction {
                    // Save needs category info now, which is inside the metadata
                    let registry_immut = registry.as_ref(); // Get immutable ref
                    if let Some(data_to_save) = registry_immut.get_sheet(&category, &sheet_name_candidate) {
                        if let Some(meta_to_save) = &data_to_save.metadata {
                             save_single_sheet(registry_immut, meta_to_save); // Pass metadata
                        }
                    }
                }
            }
            Err(e) => {
                if e.to_lowercase().contains("file is empty") {
                    info!("Startup Scan: Grid file '{}' for '{}' is empty. Registering with empty grid.", grid_path.display(), sheet_name_candidate);
                    let final_metadata = loaded_metadata.unwrap_or_else(|| {
                        needs_save_after_correction = true;
                        SheetMetadata::create_generic(
                            sheet_name_candidate.clone(),
                            filename.clone(), // Pass only filename
                            0,
                            category.clone() // Pass category
                        )
                    });
                    // Consistency checks done

                    let sheet_data = SheetGridData { metadata: Some(final_metadata), grid: Vec::new() };
                    registry.add_or_replace_sheet(category.clone(), sheet_name_candidate.clone(), sheet_data);
                    found_unregistered_count += 1;
                    info!("Startup Scan: Registered empty sheet '{}' category '{:?}'.", sheet_name_candidate, category);
                    if needs_save_after_correction {
                         let registry_immut = registry.as_ref();
                         if let Some(data_to_save) = registry_immut.get_sheet(&category, &sheet_name_candidate) {
                             if let Some(meta_to_save) = &data_to_save.metadata {
                                 save_single_sheet(registry_immut, meta_to_save); // Pass metadata
                             }
                         }
                    }
                } else {
                    error!("Startup Scan: Failed to load grid data from '{}' for sheet '{}': {}", grid_path.display(), sheet_name_candidate, e);
                }
            }
        }
    } // End processing loop

    if found_unregistered_count > 0 {
        info!("Startup Scan: Found and processed {} unregistered sheets.", found_unregistered_count);
    } else {
        info!("Startup Scan: No new unregistered sheets found to process.");
    }
}