// src/sheets/systems/io/mod.rs

use bevy::prelude::error;
use std::path::PathBuf;

// --- Submodule Declarations ---
pub mod load;       // Runtime uploads
pub mod save;
pub mod validator;
pub mod parsers;
pub mod startup_load; // <-- Renamed/Split
pub mod startup_scan; // <-- Added

// --- Shared Constants ---
pub const DEFAULT_DATA_DIR: &str = "data_sheets";

// --- Shared Helper Functions ---
pub fn get_default_data_base_path() -> PathBuf {
    // ... (function remains the same)
    let base_dir = if let Ok(exe_path) = std::env::current_exe() {
        exe_path.parent().map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
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
// Re-export necessary items from submodules

// Runtime Load/Upload systems (from load.rs)
pub use load::{
    handle_json_sheet_upload,
    handle_initiate_file_upload,
    handle_process_upload_request,
};
// Save systems (from save.rs)
pub use save::{
    handle_delete_sheet_file_request,
    handle_rename_sheet_file_request,
    save_single_sheet, // Keep exporting save_single_sheet, might be useful externally
};
// Startup systems (from NEW modules)
pub use startup_load::{
    register_sheet_metadata,
    load_registered_sheets_startup,
};
pub use startup_scan::{
    scan_directory_for_sheets_startup,
};