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
pub fn get_default_data_base_path() -> PathBuf {
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
pub use load::{
    register_sheet_metadata,
    load_registered_sheets_startup,
    handle_json_sheet_upload,
    handle_initiate_file_upload,
    handle_process_upload_request,
};
// Modify save exports: Export save_single_sheet, keep file ops
pub use save::{
    // Resources REMOVED
    // Systems
    handle_delete_sheet_file_request,
    handle_rename_sheet_file_request,
    // setup_autosave_state, REMOVED
    // detect_changes_and_trigger_autosave, REMOVED
    // run_autosave_if_needed, REMOVED
    // save_all_sheets_logic, // No longer the primary mechanism
    save_single_sheet, // NEW: Export the single save function
};
pub use scan::scan_directory_for_sheets_startup;