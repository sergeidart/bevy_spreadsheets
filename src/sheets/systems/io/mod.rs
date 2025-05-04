// src/sheets/systems/io/mod.rs

use bevy::prelude::error;
use std::path::PathBuf;

// --- Submodule Declarations ---
pub mod load;
pub mod save;
pub mod scan;

// --- Shared Constants ---
pub const DEFAULT_DATA_DIR: &str = "data_sheets";

// --- Shared Helper Functions ---

/// Helper to get the base path for data files (near executable).
pub fn get_default_data_base_path() -> PathBuf {
    let base_dir = if let Ok(exe_path) = std::env::current_exe() {
        exe_path.parent().map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                 // Use bevy's warn! macro
                 error!("Could not get parent directory of executable, using current working directory '.' instead.");
                 PathBuf::from(".")
            })
    } else {
        error!("Failed to get current executable path, using current working directory '.' instead.");
        PathBuf::from(".")
    };

    base_dir.join(DEFAULT_DATA_DIR)
}

// --- Public Re-exports for Plugin ---
// Re-export specific functions/systems needed by the plugin or other modules
pub use load::{
    register_sheet_metadata,
    load_registered_sheets_startup,
    handle_json_sheet_upload,
    // Optionally re-export load_and_parse_json_sheet if needed elsewhere directly
    // load_and_parse_json_sheet,
};
pub use save::handle_save_request;
pub use scan::scan_directory_for_sheets_startup;