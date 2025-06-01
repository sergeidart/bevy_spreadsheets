// src/sheets/systems/io/startup/registration.rs
use crate::{
    example_definitions::{
        create_example_items_metadata, create_simple_config_metadata,
    },
    sheets::{
        definitions::{SheetGridData, SheetMetadata},
        resources::SheetRegistry,
        systems::io::{
            get_default_data_base_path,
            save::save_single_sheet,
            validator, // Keep validator import for grid validation
        },
    },
};
use bevy::prelude::*;
use std::fs;

/// Registers default sheets if the data directory is empty or missing.
/// Creates the directory if needed. Saves the default sheets afterwards.
pub fn register_default_sheets_if_needed(mut registry: ResMut<SheetRegistry>) {
    let data_dir_path = get_default_data_base_path();
    let mut needs_creation = false;

    // Check if directory exists and is empty
    if !data_dir_path.exists() {
        info!(
            "Data directory '{:?}' is missing. Will create and register default templates.",
            data_dir_path
        );
        needs_creation = true;
    } else if fs::read_dir(&data_dir_path)
        .map_or(true, |mut dir| dir.next().is_none())
    {
        info!(
            "Data directory '{:?}' is empty. Registering default template sheets.",
            data_dir_path
        );
        needs_creation = true;
    }

    if needs_creation {
        // Attempt to create directory if it doesn't exist
        if !data_dir_path.exists() {
            if let Err(e) = fs::create_dir_all(&data_dir_path) {
                error!(
                    "Failed to create data directory {:?}: {}. Cannot register or save template sheets.",
                    data_dir_path, e
                );
                return; // Cannot proceed without data directory
            }
        }

        // Create metadata for default sheets
        let example_meta = create_example_items_metadata();
        let config_meta = create_simple_config_metadata();

        // Register metadata in the registry
        let registered_example = registry.register(example_meta.clone());
        let registered_config = registry.register(config_meta.clone());

        // Save the newly registered sheets
        if registered_example || registered_config {
            info!("Registered pre-defined template sheet metadata.");
            let registry_immut = &*registry; // Immutable borrow for saving
            if registered_example {
                info!("Saving template sheet: {}", example_meta.sheet_name);
                save_single_sheet(registry_immut, &example_meta);
            }
            if registered_config {
                info!("Saving template sheet: {}", config_meta.sheet_name);
                save_single_sheet(registry_immut, &config_meta);
            }
        }
    } else {
        info!(
            "Data directory '{:?}' already exists and is not empty. Skipping registration of default template sheets.",
            data_dir_path
        );
    }
}

/// Validates the grid structure against metadata and adds the sheet to the registry.
/// Called by the scan process after metadata and grid data have been loaded.
/// Returns true if registration was successful, false otherwise.
pub(super) fn add_scanned_sheet_to_registry(
    registry: &mut SheetRegistry, // Needs mutable access
    category: Option<String>,
    sheet_name: String,
    metadata: SheetMetadata, // Takes ownership of finalized metadata
    grid: Vec<Vec<String>>,  // Takes ownership of loaded grid
    source_path_display: String, // For logging
) -> bool {
    // --- Final Grid Structure Validation ---
    if let Err(e) =
        validator::validate_grid_structure(&grid, &metadata, &sheet_name)
    {
        error!(
            "Grid structure validation failed for '{}' during registration: {}. Skipping registration.",
            source_path_display, e
        );
        return false; // Do not register if grid structure is invalid
    }

    // --- Registration ---
    let sheet_data = SheetGridData {
        metadata: Some(metadata), // Store the metadata
        grid,                     // Store the grid
    };

    registry.add_or_replace_sheet(category.clone(), sheet_name.clone(), sheet_data);
    info!(
        "Registered sheet '{:?}/{}' from file '{}'.",
        category, sheet_name, source_path_display
    );
    true
}

