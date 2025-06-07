// src/visual_copier/executers.rs

use bevy::prelude::*;
use std::path::PathBuf;
use chrono::Local;
use fs_extra; // Ensure fs_extra is a dependency in Cargo.toml

use super::resources::CopyError;

/// Helper function to execute a single copy operation (blocking).
pub(crate) fn execute_single_copy_operation(
    from_path: &PathBuf,
    to_path: &PathBuf,
    operation_label: &str,
) -> Result<String, CopyError> {
    debug!(
        "Executing copy: {} -> {} for {}",
        from_path.display(),
        to_path.display(),
        operation_label
    );

    if !from_path.exists() {
        return Err(CopyError::SourceDoesNotExist(from_path.clone()));
    }
    if !from_path.is_dir() {
        return Err(CopyError::StartNotADirectory(from_path.clone()));
    }

    // Ensure the target directory exists, creating it if necessary.
    // This also helps validate if the to_path is usable.
    if let Err(_e) = std::fs::create_dir_all(to_path) {
        // More specific error if creating the directory fails.
        return Err(CopyError::EndPathInvalid(to_path.clone()));
    }


    let mut options = fs_extra::dir::CopyOptions::new();
    options.overwrite = true; // Overwrite existing files/folders.
    options.content_only = true; // Copy the content of the source folder, not the folder itself.

    // Perform the copy operation.
    // fs_extra::dir::copy returns a Result, map its error to your CopyError type.
    fs_extra::dir::copy(from_path, to_path, &options).map_err(CopyError::from)?;

    let success_msg = format!(
        "{} copied successfully at {}",
        operation_label,
        Local::now().format("%H:%M:%S")
    );
    info!("VisualCopier: {}", success_msg);
    Ok(success_msg)
}