// src/sheets/systems/io/mod.rs

use bevy::prelude::{error, trace}; // <<< Added trace macro
use std::path::{PathBuf, Path};
use crate::sheets::definitions::SheetMetadata; // <<< Added SheetMetadata import

// --- Submodule Declarations ---
pub mod load;       // Runtime uploads
pub mod save;
pub mod validator;
pub mod parsers;
pub mod startup_load;
pub mod startup_scan;

// --- Shared Constants ---
pub const DEFAULT_DATA_DIR: &str = "data_sheets";

// --- Shared Helper Functions ---

/// Gets the absolute base path for the data_sheets directory.
pub fn get_default_data_base_path() -> PathBuf {
    let base_dir = if let Ok(exe_path) = std::env::current_exe() {
        exe_path.parent().map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        error!("Failed to get current executable path, using current working directory '.' instead.");
        PathBuf::from(".")
    };
    let data_path = base_dir.join(DEFAULT_DATA_DIR);
    trace!("Data base path determined as: {:?}", data_path); // <<< trace! should now work
    data_path
}

/// Helper to get the full path for a sheet file given its metadata.
// SheetMetadata should now be found >>>
pub fn get_full_sheet_path(base_data_path: &Path, metadata: &SheetMetadata) -> PathBuf {
    let mut path = base_data_path.to_path_buf();
    if let Some(cat) = &metadata.category {
        path.push(cat);
    }
    path.push(&metadata.data_filename);
    path
}

/// Helper to get the full path for a sheet's metadata file.
// SheetMetadata should now be found >>>
pub fn get_full_metadata_path(base_data_path: &Path, metadata: &SheetMetadata) -> PathBuf {
     let mut path = base_data_path.to_path_buf();
     if let Some(cat) = &metadata.category {
         path.push(cat);
     }
     path.push(format!("{}.meta.json", metadata.sheet_name)); // Meta file uses sheet name
     path
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
    save_single_sheet, // Keep exporting save_single_sheet
};
// Startup systems (from NEW modules)
pub use startup_load::{
    register_sheet_metadata,
    load_registered_sheets_startup,
};
pub use startup_scan::{
    scan_directory_for_sheets_startup,
};