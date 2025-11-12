// src/sheets/systems/io/mod.rs

use crate::sheets::definitions::SheetMetadata;
use bevy::prelude::{error, trace};
use std::path::{Path, PathBuf};

// --- Submodule Declarations ---
pub mod load; // Runtime uploads
pub mod lazy_load; // Lazy loading of database tables
pub mod metadata_persistence;
pub mod parsers;
pub mod save;
pub mod startup;
pub mod validator; // <-- ADDED new startup submodule

// --- Shared Constants ---
// Legacy constant kept for reference, but default path now uses Documents/SkylineDB
pub const DEFAULT_DATA_DIR: &str = "SkylineDB";

// --- Shared Helper Functions ---
/// Get the default data base path for JSON sheets.
/// Now uses Documents/SkylineDB to match the database location.
pub fn get_default_data_base_path() -> PathBuf {
    let data_path = directories_next::UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| {
            error!(
                "Failed to get Documents directory, using current working directory '.' instead."
            );
            PathBuf::from(".")
        })
        .join(DEFAULT_DATA_DIR);

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
    handle_initiate_file_upload, handle_json_sheet_upload, handle_process_upload_request,
};

pub use save::{
    handle_delete_sheet_file_request, handle_rename_sheet_file_request,
};