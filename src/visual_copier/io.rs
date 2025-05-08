// src/visual_copier/io.rs

use super::resources::CopyTask;
use directories_next::ProjectDirs; // Ensure this crate is added to Cargo.toml
use std::fs;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use bevy::prelude::{error, info}; // Use bevy's logging macros

// Constants for configuration directory and file
const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "BevyAppOrg";
const APPLICATION: &str = "BevyVisualCopierApp";
const CONFIG_FILE: &str = "visual_copier_tasks.json";

/// Helper function to get the configuration file path.
fn get_config_path() -> io::Result<PathBuf> {
    if let Some(proj_dirs) = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION) {
        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir)?; // Ensure the config directory exists
        let config_file_path = config_dir.join(CONFIG_FILE);
        Ok(config_file_path)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine project directories for VisualCopier config.",
        ))
    }
}

/// Loads copy tasks from the configuration file.
pub fn load_copy_tasks_from_file() -> io::Result<Vec<CopyTask>> {
    let config_file = get_config_path()?;

    if !config_file.exists() {
        info!("VisualCopier: Config file not found at {:?}. Starting with no tasks.", config_file);
        return Ok(Vec::new()); // No file, so no tasks
    }

    info!("VisualCopier: Loading copy tasks from {:?}", config_file);
    let file = fs::File::open(&config_file)?; // Borrow config_file here
    let reader = BufReader::new(file);

    match serde_json::from_reader(reader) {
        Ok(tasks) => Ok(tasks),
        Err(e) => {
            // Borrow config_file for the error message
            error!("VisualCopier: Error loading or parsing config file {:?}: {}. Starting with no tasks.", &config_file, e);
            Ok(Vec::new()) // Return default (empty Vec) on error
        }
    }
}

/// Saves copy tasks to the configuration file.
pub fn save_copy_tasks_to_file(copy_tasks: &[CopyTask]) -> io::Result<()> {
    let config_file = get_config_path()?;
    info!("VisualCopier: Saving {} copy tasks to {:?}", copy_tasks.len(), &config_file); // Borrow config_file

    let file = fs::File::create(&config_file)?; // Borrow config_file here
    let writer = BufWriter::new(file);

    // Use map_err to handle potential serialization errors
    serde_json::to_writer_pretty(writer, copy_tasks).map_err(|e| {
        // Borrow config_file for the error message
        error!("VisualCopier: Failed to serialize copy tasks to {:?}: {}", &config_file, e);
        io::Error::new(io::ErrorKind::Other, e) // Convert serde error to io::Error
    })?;

    Ok(())
}