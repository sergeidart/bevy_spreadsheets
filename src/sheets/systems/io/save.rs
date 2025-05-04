// src/sheets/systems/io/save.rs

use bevy::prelude::*;
use std::{
    fs::{self, File},
    io::BufWriter,
};

// Use items defined in the parent io module (io/mod.rs)
use super::get_default_data_base_path;

// Use types from the main sheets module
use crate::sheets::{
    events::RequestSaveSheets,
    resources::SheetRegistry,
};

/// Core logic function to save all registered sheets to JSON files.
fn save_all_sheets_logic(registry: Res<SheetRegistry>) {
    info!("Attempting to save all sheets...");
    let base_path = get_default_data_base_path();

    // Ensure the base directory exists
    if let Err(e) = fs::create_dir_all(&base_path) {
        error!("Failed to create data directory '{:?}' for saving: {}. Aborting save.", base_path, e);
        return;
    }

    let mut saved_count = 0;
    let mut error_count = 0;

    for (sheet_name, sheet_data) in registry.iter_sheets() {
        if let Some(meta) = &sheet_data.metadata {
            let filename = &meta.data_filename;
            if filename.is_empty() {
                 warn!("Skipping save for sheet '{}' due to empty filename.", sheet_name);
                 continue;
            }

            let full_path = base_path.join(filename);
            trace!("Saving sheet '{}' to '{}'...", sheet_name, full_path.display()); // Use trace

            match File::create(&full_path) {
                Ok(file) => {
                    let writer = BufWriter::new(file);
                    match serde_json::to_writer_pretty(writer, &sheet_data.grid) {
                        Ok(_) => {
                             // BufWriter flushes on drop, log success after match
                            saved_count += 1;
                        }
                        Err(e) => {
                            error!("Failed to serialize sheet '{}' to JSON: {}", sheet_name, e);
                            error_count += 1;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to create/open file '{}' for sheet '{}': {}", full_path.display(), sheet_name, e);
                    error_count += 1;
                }
            }
        } else {
            warn!("Skipping save for sheet '{}': No metadata.", sheet_name);
        }
    }

    if error_count > 0 {
         error!("Finished saving sheets with {} errors.", error_count);
    } else if saved_count > 0 {
        info!("Successfully saved {} sheets.", saved_count);
    } else {
        info!("No sheets were saved.");
    }
}

/// Handles the `RequestSaveSheets` event sent from the UI. (Update system)
pub fn handle_save_request(
    mut events: EventReader<RequestSaveSheets>,
    registry: Res<SheetRegistry>, // Needs read access
) {
    if !events.is_empty() {
        info!("Save request received via event.");
        events.clear(); // Consume the event(s)
        save_all_sheets_logic(registry); // Call the core logic
    }
}