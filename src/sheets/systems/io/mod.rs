// src/sheets/systems/io/mod.rs

use bevy::prelude::{error, trace};
use std::path::{Path, PathBuf};
use crate::sheets::definitions::SheetMetadata;

// --- Submodule Declarations ---
pub mod load; // Runtime uploads
pub mod save;
pub mod validator;
pub mod parsers;
pub mod startup; // <-- ADDED new startup submodule

// --- Shared Constants ---
pub const DEFAULT_DATA_DIR: &str = "data_sheets";

// --- Shared Helper Functions ---
// (These remain the same)
pub fn get_default_data_base_path() -> PathBuf {
    let base_dir = if let Ok(exe_path) = std::env::current_exe() {
        exe_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        error!("Failed to get current executable path, using current working directory '.' instead.");
        PathBuf::from(".")
    };
    let data_path = base_dir.join(DEFAULT_DATA_DIR);
    trace!("Data base path determined as: {:?}", data_path);
    data_path
}
pub fn get_full_sheet_path(base_data_path: &Path, metadata: &SheetMetadata) -> PathBuf {
    let mut path = base_data_path.to_path_buf();
    if let Some(cat) = &metadata.category {
        path.push(cat);
    }
    path.push(&metadata.data_filename);
    path
}
pub fn get_full_metadata_path(base_data_path: &Path, metadata: &SheetMetadata) -> PathBuf {
    let mut path = base_data_path.to_path_buf();
    if let Some(cat) = &metadata.category {
        path.push(cat);
    }
    path.push(format!("{}.meta.json", metadata.sheet_name));
    path
}

pub use load::{
    handle_json_sheet_upload, handle_initiate_file_upload,
    handle_process_upload_request,
};
// Save systems (from save.rs)
pub use save::{
    handle_delete_sheet_file_request, handle_rename_sheet_file_request
};