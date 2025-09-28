// src/sheets/systems/io/startup/metadata_load.rs
use crate::sheets::{
    definitions::SheetMetadata,
    systems::io::{
        parsers::read_and_parse_metadata_file,
        validator, // Import the validator module
    },
};
use bevy::prelude::*;
use std::path::Path;

pub(super) fn load_and_validate_metadata_file(
    base_path: &Path,
    relative_grid_path: &Path,
    sheet_name_candidate: &str,
    category: &Option<String>,
    full_grid_path: &Path, // Needed for filename correction
) -> Result<(Option<SheetMetadata>, bool), String> {
    // Construct the expected path to the metadata file
    let expected_meta_path = base_path
        .join(relative_grid_path.with_file_name(format!("{}.meta.json", sheet_name_candidate)));

    let mut loaded_metadata: Option<SheetMetadata> = None;
    let mut needs_correction_save = false;

    // Check if the metadata file exists
    if expected_meta_path.exists() {
        match read_and_parse_metadata_file(&expected_meta_path) {
            Ok(mut meta) => {
                // Get the actual filename from the full grid path for validation
                let actual_grid_filename = match full_grid_path.file_name() {
                    Some(os) => os.to_string_lossy().into_owned(),
                    None => {
                        // This should be unlikely if full_grid_path is valid
                        error!(
                            "Could not extract filename from '{}' during metadata load.",
                            full_grid_path.display()
                        );
                        // Return Ok(None) as we cannot reliably validate/correct filename
                        return Ok((None, false));
                    }
                };

                // Validate and correct loaded metadata against derived/expected values
                // Use warning mode to allow corrections.
                match validator::validate_or_correct_loaded_metadata(
                    &mut meta,
                    sheet_name_candidate,
                    &actual_grid_filename, // Use the actual grid filename here
                    true,                  // Use warning mode to correct
                ) {
                    Ok(()) => {
                        // Basic validation/correction passed
                        // Further ensure category matches folder structure
                        if meta.category != *category {
                            warn!(
                                "Correcting category in '{}' from '{:?}' to match folder structure '{:?}'.",
                                expected_meta_path.display(), meta.category, category
                            );
                            meta.category = category.clone();
                            needs_correction_save = true;
                        }
                        // Ensure internal column definitions are consistent
                        if meta.ensure_column_consistency() {
                            info!(
                                "Corrected column consistency for loaded metadata of '{}'.",
                                sheet_name_candidate
                            );
                            needs_correction_save = true;
                        }
                        trace!(
                            "Loaded and validated metadata for '{}' from '{}'. Corrections applied: {}",
                            sheet_name_candidate,
                            expected_meta_path.display(),
                            needs_correction_save
                        );
                        loaded_metadata = Some(meta); // Store the validated/corrected metadata
                    }
                    Err(e) => {
                        // This block shouldn't be reached in warnings_only mode unless validator changes
                        error!(
                            "Metadata validation failed unexpectedly for '{}': {}",
                            sheet_name_candidate, e
                        );
                        // Return Ok but with None metadata and no correction flag set
                        return Ok((None, false));
                    }
                }
            }
            Err(e) => {
                // Failed to read/parse the existing metadata file
                error!(
                    "Failed to load/parse metadata file '{}' for sheet '{}': {}. Proceeding without loaded metadata.",
                    expected_meta_path.display(), sheet_name_candidate, e
                );
                // Proceed without loaded metadata, might generate default later
            }
        }
    } else {
        // Metadata file doesn't exist
        trace!(
            "Metadata file '{}' not found for '{}'. Will generate default if grid loads.",
            expected_meta_path.display(),
            sheet_name_candidate
        );
    }

    // Return the loaded metadata (or None) and the correction flag
    Ok((loaded_metadata, needs_correction_save))
}

// Potential future function (if needed for loading registered sheets differently)
// pub(super) fn load_metadata_for_registered_sheet(...) -> Result<Option<SheetMetadata>, String> { ... }
