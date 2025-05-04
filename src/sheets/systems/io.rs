// src/sheets/systems/io.rs

use bevy::prelude::*;
use std::{
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::PathBuf,
};

// *** Import metadata from the app's example definitions ***
use crate::example_definitions::{EXAMPLE_ITEMS_METADATA, SIMPLE_CONFIG_METADATA};
// *** ***

use super::super::{ // Use types from parent (crate root)
    resources::SheetRegistry,
    definitions::SheetGridData,
    events::RequestSaveSheets,
};

pub const DEFAULT_DATA_DIR: &str = "data_sheets";

/// Startup system to register all known sheet metadata for *this app*.
/// Must run before `load_all_sheets_startup`.
pub fn register_sheet_metadata(mut registry: ResMut<SheetRegistry>) {
    // Register directly using constants from example_definitions.rs
    registry.register(EXAMPLE_ITEMS_METADATA);
    registry.register(SIMPLE_CONFIG_METADATA);
}


/// Startup system to load ALL registered sheets from JSON files.
/// Assumes metadata has already been registered.
pub fn load_all_sheets_startup(mut registry: ResMut<SheetRegistry>) {
    let base_path = get_default_data_base_path();

    if let Err(e) = fs::create_dir_all(&base_path) {
    }

    let sheet_names: Vec<&'static str> = registry.get_sheet_names().clone();
    if sheet_names.is_empty() {
        return;
    }

    for sheet_name in sheet_names {
        let filename_to_load = match registry.get_sheet(sheet_name).and_then(|d| d.metadata.as_ref().map(|m| m.data_filename)) {
            Some(fname) => fname,
            None => {
                continue;
            }
        };

        let full_path = base_path.join(filename_to_load);

        match File::open(&full_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                match serde_json::from_reader::<_, Vec<Vec<String>>>(reader) {
                    Ok(grid_data) => {
                        if let Some(sheet_entry) = registry.get_sheet_mut(sheet_name) {
                            sheet_entry.grid = grid_data;
                        } else {
                        }
                    }
                    Err(e) => {
                    }
                }
            }
            Err(e) => {
                 if let Some(sheet_entry) = registry.get_sheet_mut(sheet_name) {
                     if !sheet_entry.grid.is_empty() {
                        sheet_entry.grid.clear();
                     }
                 }
            }
        }
    }
}


/// Core logic function to save all registered sheets to JSON files.
pub fn save_all_sheets_logic(registry: Res<SheetRegistry>) {
    let base_path = get_default_data_base_path();

    if let Err(e) = fs::create_dir_all(&base_path) {
        return;
    }

    for (sheet_name, sheet_data) in registry.iter_sheets() {
        if let Some(meta) = &sheet_data.metadata {
            let filename = meta.data_filename;
            let full_path = base_path.join(filename);

        } else {
        }
    }
}

/// Handles the `RequestSaveSheets` event sent from the UI.
pub fn handle_save_request(
    mut events: EventReader<RequestSaveSheets>,
    registry: Res<SheetRegistry>, // Needs read access to pass to save logic
) {
    if !events.is_empty() {
        events.clear();
        save_all_sheets_logic(registry); // Call the core logic
    }
}

/// Helper to get the base path for data files (near executable).
pub fn get_default_data_base_path() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(DEFAULT_DATA_DIR)
}