// src/sheets/systems/io/startup/scan.rs
use crate::sheets::{
    definitions::{SheetGridData, SheetMetadata},
    resources::SheetRegistry,
    systems::io::{
        get_default_data_base_path,
        save::save_single_sheet,
        validator, // Import validator module
        // Import specific startup modules needed
        startup::{metadata_load, grid_load, registration},
    },
};
use bevy::prelude::*;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn scan_filesystem_for_unregistered_sheets(
    mut registry: ResMut<SheetRegistry>,
) {
    let base_path = get_default_data_base_path();
    info!(
        "Startup Scan: Recursively scanning directory '{:?}' for sheets...",
        base_path
    );

    if !base_path.exists() {
        info!("Startup Scan: Data directory does not exist. Nothing to scan.");
        return;
    }

    let mut found_unregistered_count = 0;
    let mut potential_grid_files = Vec::new();

    for entry_result in WalkDir::new(&base_path)
        .min_depth(1) // Skip the base directory itself
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry_result.path();
        let is_json = path
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("json"));
        let is_meta_file = path
            .file_name()
            .map_or(false, |name| {
                name.to_string_lossy().ends_with(".meta.json")
            });

        // Only consider non-meta JSON files
        if is_json && !is_meta_file {
            potential_grid_files.push(path.to_path_buf());
        }
    }

    if potential_grid_files.is_empty() {
        info!("Startup Scan: No potential unregistered grid files (.json) found.");
        return;
    } else {
        trace!("Found potential grid files: {:?}", potential_grid_files);
    }

    for grid_path in potential_grid_files {
        let filename = grid_path.file_name().map_or_else(
            || "unknown.json".to_string(),
            |os| os.to_string_lossy().into_owned(),
        );

        // Derive sheet name candidate from file stem
        let sheet_name_candidate = grid_path.file_stem().map_or_else(
            || {
                filename
                    .trim_end_matches(".json")
                    .trim_end_matches(".JSON")
                    .to_string()
            },
            |os| os.to_string_lossy().into_owned(),
        );

        // Derive relative path and category
        let relative_path = match grid_path.strip_prefix(&base_path) {
            Ok(rel) => rel,
            Err(_) => {
                error!(
                    "Failed to strip base path from '{}'. Skipping.",
                    grid_path.display()
                );
                continue;
            }
        };
        let category: Option<String> = relative_path
            .parent()
            .and_then(|p| p.file_name())
            .map(|os_str| os_str.to_string_lossy().into_owned())
            .filter(|dir_name| !dir_name.is_empty()); // Ensure parent is not root

        trace!(
            "Processing '{}'. Derived Name: '{}'. Derived Category: '{:?}'",
            grid_path.display(),
            sheet_name_candidate,
            category
        );

        // --- Validation using validator module ---
        if let Err(e) = validator::validate_derived_sheet_name(&sheet_name_candidate) {
            warn!(
                "Skipping file '{}': Validation failed: {}",
                grid_path.display(), e
            );
            continue;
        }
        if let Some(cat_name) = category.as_deref() {
            if let Err(e) = validator::validate_derived_category_name(cat_name) {
                warn!(
                    "Skipping file '{}' in category '{:?}': Validation failed: {}",
                    grid_path.display(), category, e
                );
                continue;
            }
        }
        // --- End Validation ---

        // Check if already registered (considering category)
        let already_registered_name =
            registry.does_sheet_exist(&sheet_name_candidate);
        let already_registered_file = registry
            .get_sheet(&category, &sheet_name_candidate)
            .map_or(false, |data| {
                data.metadata
                    .as_ref()
                    .map_or(false, |m| m.data_filename == filename)
            });

        if already_registered_name && !already_registered_file {
            warn!("Skipping file '{}' (Sheet name '{}' already exists, possibly in another category or with a different filename). Manual resolution required.", grid_path.display(), sheet_name_candidate);
            continue;
        }
        if already_registered_file {
            trace!("Skipping file '{}': Sheet '{}' in category '{:?}' seems already registered.", grid_path.display(), sheet_name_candidate, category);
            continue;
        }

        trace!(
            "Found potential unregistered grid file: '{}'. Attempting load as sheet '{}' in category '{:?}'.",
            filename, sheet_name_candidate, category
        );

        // --- Phase 3: Load Metadata and Grid Data ---
        let mut needs_metadata_save = false; // Flag if metadata needs saving later

        // 3a. Load Metadata (using metadata_load module)
        let meta_load_result = metadata_load::load_and_validate_metadata_file(
            &base_path,
            relative_path, // Pass relative path to grid file
            &sheet_name_candidate,
            &category,
            &grid_path, // Pass full grid path for filename correction reference
        );
        let mut loaded_metadata = match meta_load_result {
            Ok((meta_opt, corrected)) => {
                if corrected { needs_metadata_save = true; }
                meta_opt
            }
            Err(e) => { // Should not happen based on current implementation, but handle defensively
                error!("Metadata loading failed unexpectedly for sheet '{}': {}. Will generate default.", sheet_name_candidate, e);
                None // Proceed with default generation later
            }
        };

        // 3b. Load Grid Data (using grid_load module)
        let grid_load_result = grid_load::load_grid_data_file(&grid_path);
        let final_grid: Vec<Vec<String>>; // Store final grid data here

        match grid_load_result {
            Ok(Some(grid)) => { // Grid file found and parsed
                final_grid = grid;
            }
            Ok(None) => { // Grid file was empty
                info!(
                    "Grid file '{}' for '{}' is empty. Using empty grid.",
                    grid_path.display(),
                    sheet_name_candidate
                );
                final_grid = Vec::new(); // Use empty grid
            }
            Err(e) => { // Error loading/parsing grid file
                error!(
                    "Failed to load grid data from '{}' for sheet '{}': {}",
                    grid_path.display(),
                    sheet_name_candidate,
                    e
                );
                continue; // Skip this file entirely if grid fails to load
            }
        }

        // --- Phase 4: Finalize Metadata and Register ---
        // Finalize metadata (use loaded or generate default)
        let final_metadata = loaded_metadata.take().unwrap_or_else(|| {
            info!(
                "Generating default metadata for sheet '{}' category '{:?}'.",
                sheet_name_candidate, category
            );
            needs_metadata_save = true; // Mark for saving if generated
            let num_cols = final_grid.first().map_or(0, |r| r.len());
            SheetMetadata::create_generic(
                sheet_name_candidate.clone(),
                filename.clone(), // Use derived filename
                num_cols,
                category.clone(),
            )
        });

        // --- Registration (using registration module) ---
        // Pass mutable registry ref, category, name, and *owned* metadata/grid
        if registration::add_scanned_sheet_to_registry(
            &mut registry, // Pass mutable ref
            category.clone(),
            sheet_name_candidate.clone(),
            final_metadata.clone(), // Clone metadata for registration
            final_grid, // Pass ownership of grid
            grid_path.display().to_string(), // Pass display path for logging
        ) {
            // If registration was successful
            found_unregistered_count += 1;

            // --- Phase 5: Save Corrected/Generated Metadata ---
            if needs_metadata_save {
                let registry_immut = registry.as_ref(); // Immutable borrow for save
                // Use the final_metadata directly which is owned
                trace!(
                    "Saving corrected/generated metadata for '{:?}/{}'",
                    category, sheet_name_candidate
                );
                save_single_sheet(registry_immut, &final_metadata);
            }
        }
        // If registration failed (e.g., grid validation), the error is logged within add_scanned_sheet_to_registry
    } // End processing loop for potential grid files
    if found_unregistered_count > 0 {
        info!(
            "Startup Scan: Found and processed {} unregistered sheets.",
            found_unregistered_count
        );
    } else {
        info!("Startup Scan: No new unregistered sheets found to process.");
    }
}
