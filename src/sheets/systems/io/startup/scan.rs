// src/sheets/systems/io/startup/scan.rs
use crate::sheets::{
    definitions::SheetMetadata,
    events::RequestSheetRevalidation,
    resources::SheetRegistry,
    systems::io::{
        get_default_data_base_path,
        save::save_single_sheet,
        startup::{grid_load, metadata_load, registration},
        validator,
    },
};
use bevy::prelude::*;
use walkdir::WalkDir;

pub fn scan_filesystem_for_unregistered_sheets(
    mut registry: ResMut<SheetRegistry>,
    // ADDED revalidation writer
    mut revalidate_writer: EventWriter<RequestSheetRevalidation>,
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
    // ADDED: Track successfully registered sheets for validation
    let mut sheets_registered_in_scan = Vec::new();

    // --- Also collect empty category directories so they appear even without sheets ---
    let mut empty_dirs: Vec<String> = Vec::new();
    for entry_result in WalkDir::new(&base_path)
        .min_depth(1)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
    {
        let entry = entry_result;
        if entry.depth() == 1 && entry.file_type().is_dir() {
            let dir_path = entry.path();
            // consider it a category dir if it has no files with .json inside
            let mut has_any_json = false;
            if let Ok(mut rd) = std::fs::read_dir(dir_path) {
                while let Some(Ok(child)) = rd.next() {
                    if child.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        if child
                            .path()
                            .extension()
                            .map_or(false, |ext| ext.eq_ignore_ascii_case("json"))
                        {
                            has_any_json = true;
                            break;
                        }
                    }
                }
            }
            if !has_any_json {
                if let Some(name_os) = dir_path.file_name() {
                    let name = name_os.to_string_lossy().to_string();
                    // Registrar: push if valid category name and not already implicitly present
                    if registry
                        .get_sheet_names_in_category(&Some(name.clone()))
                        .is_empty()
                    {
                        empty_dirs.push(name);
                    }
                }
            }
        }
    }

    // Register explicit empty categories so they show up
    for cat_name in empty_dirs {
        // Use registry API which avoids duplicates
        let _ = registry.create_category(cat_name);
    }

    // --- (Finding potential grid files remains the same) ---
    for entry_result in WalkDir::new(&base_path)
        .min_depth(1)
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
            .map_or(false, |name| name.to_string_lossy().ends_with(".meta.json"));
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
        // --- (Deriving name, category, validation remains the same) ---
        let filename = grid_path.file_name().map_or_else(
            || "unknown.json".to_string(),
            |os| os.to_string_lossy().into_owned(),
        );
        let sheet_name_candidate = grid_path.file_stem().map_or_else(
            || {
                filename
                    .trim_end_matches(".json")
                    .trim_end_matches(".JSON")
                    .to_string()
            },
            |os| os.to_string_lossy().into_owned(),
        );
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
            .filter(|dir_name| !dir_name.is_empty());
        trace!(
            "Processing '{}'. Derived Name: '{}'. Derived Category: '{:?}'",
            grid_path.display(),
            sheet_name_candidate,
            category
        );
        if let Err(e) = validator::validate_derived_sheet_name(&sheet_name_candidate) {
            warn!(
                "Skipping file '{}': Validation failed: {}",
                grid_path.display(),
                e
            );
            continue;
        }
        if let Some(cat_name) = category.as_deref() {
            if let Err(e) = validator::validate_derived_category_name(cat_name) {
                warn!(
                    "Skipping file '{}' in category '{:?}': Validation failed: {}",
                    grid_path.display(),
                    category,
                    e
                );
                continue;
            }
        }
        let already_registered_name = registry.does_sheet_exist(&sheet_name_candidate);
        let already_registered_file =
            registry
                .get_sheet(&category, &sheet_name_candidate)
                .map_or(false, |data| {
                    data.metadata
                        .as_ref()
                        .map_or(false, |m| m.data_filename == filename)
                });
        if already_registered_name && !already_registered_file {
            warn!("Skipping file '{}' (Sheet name '{}' already exists, possibly in another category or with a different filename).", grid_path.display(), sheet_name_candidate);
            continue;
        }
        if already_registered_file {
            trace!(
                "Skipping file '{}': Sheet '{}' in category '{:?}' seems already registered.",
                grid_path.display(),
                sheet_name_candidate,
                category
            );
            continue;
        }
        trace!("Found potential unregistered grid file: '{}'. Attempting load as sheet '{}' in category '{:?}'.", filename, sheet_name_candidate, category );

        // --- (Metadata and Grid Loading remains the same) ---
        let mut needs_metadata_save = false;
        let meta_load_result = metadata_load::load_and_validate_metadata_file(
            &base_path,
            relative_path,
            &sheet_name_candidate,
            &category,
            &grid_path,
        );
        let mut loaded_metadata = match meta_load_result {
            Ok((meta_opt, corrected)) => {
                if corrected {
                    needs_metadata_save = true;
                }
                meta_opt
            }
            Err(e) => {
                error!("Metadata loading failed unexpectedly for sheet '{}': {}. Will generate default.", sheet_name_candidate, e);
                None
            }
        };
        let grid_load_result = grid_load::load_grid_data_file(&grid_path);
        let final_grid: Vec<Vec<String>>;
        match grid_load_result {
            Ok(Some(grid)) => {
                final_grid = grid;
            }
            Ok(None) => {
                info!(
                    "Grid file '{}' for '{}' is empty. Using empty grid.",
                    grid_path.display(),
                    sheet_name_candidate
                );
                final_grid = Vec::new();
            }
            Err(e) => {
                error!(
                    "Failed to load grid data from '{}' for sheet '{}': {}",
                    grid_path.display(),
                    sheet_name_candidate,
                    e
                );
                continue;
            }
        }
        let final_metadata = loaded_metadata.take().unwrap_or_else(|| {
            info!(
                "Generating default metadata for sheet '{}' category '{:?}'.",
                sheet_name_candidate, category
            );
            needs_metadata_save = true;
            let num_cols = final_grid.first().map_or(0, |r| r.len());
            SheetMetadata::create_generic(
                sheet_name_candidate.clone(),
                filename.clone(),
                num_cols,
                category.clone(),
            )
        });

        // --- Registration ---
        if registration::add_scanned_sheet_to_registry(
            &mut registry,
            category.clone(),
            sheet_name_candidate.clone(),
            final_metadata.clone(), // Clone for registration
            final_grid,             // Pass ownership of grid
            grid_path.display().to_string(),
        ) {
            // If registration was successful
            found_unregistered_count += 1;
            // ADDED: Track for validation
            sheets_registered_in_scan.push((category.clone(), sheet_name_candidate.clone()));

            // --- (Save Corrected/Generated Metadata remains the same) ---
            if needs_metadata_save {
                let registry_immut = registry.as_ref();
                trace!(
                    "Saving corrected/generated metadata for '{:?}/{}'",
                    category,
                    sheet_name_candidate
                );
                save_single_sheet(registry_immut, &final_metadata);
            }
        }
    } // End processing loop

    if found_unregistered_count > 0 {
        info!(
            "Startup Scan: Found and processed {} unregistered sheets.",
            found_unregistered_count
        );
        // ADDED: Trigger validation for sheets registered during scan
        for (cat, name) in sheets_registered_in_scan {
            revalidate_writer.write(RequestSheetRevalidation {
                category: cat,
                sheet_name: name,
            });
        }
    } else {
        info!("Startup Scan: No new unregistered sheets found to process.");
    }
}
