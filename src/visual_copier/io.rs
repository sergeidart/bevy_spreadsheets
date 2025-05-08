// src/visual_copier/io.rs

// Use the Manager struct
use super::resources::VisualCopierManager;
use directories_next::ProjectDirs;
use std::fs;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use bevy::prelude::{error, info, warn}; // Use warn for specific cases

// Constants remain the same
const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "BevyAppOrg";
const APPLICATION: &str = "BevyVisualCopierApp";
const CONFIG_FILE: &str = "visual_copier_manager_state.json"; // Renamed file

/// Helper function to get the configuration file path.
// (get_config_path function remains the same)
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


/// Loads the VisualCopierManager state from the configuration file.
/// Returns a default manager if the file doesn't exist or fails to parse.
pub fn load_copier_manager_from_file() -> io::Result<VisualCopierManager> {
    let config_file = get_config_path()?;

    if !config_file.exists() {
        info!("VisualCopier: Config file not found at {:?}. Starting with default state.", config_file);
        return Ok(VisualCopierManager::default()); // Return default manager
    }

    info!("VisualCopier: Loading copier state from {:?}", config_file);
    let file = fs::File::open(&config_file)?;
    let reader = BufReader::new(file);

    match serde_json::from_reader(reader) {
        Ok(manager) => Ok(manager),
        Err(e) => {
            // Log error but still return a default manager
            error!("VisualCopier: Error loading or parsing config file {:?}: {}. Starting with default state.", &config_file, e);
            Ok(VisualCopierManager::default()) // Return default manager on error
        }
    }
}

/// Saves the VisualCopierManager state to the configuration file.
pub fn save_copier_manager_to_file(manager: &VisualCopierManager) -> io::Result<()> {
    let config_file = get_config_path()?;
    info!("VisualCopier: Saving {} tasks and top panel paths to {:?}", manager.copy_tasks.len(), &config_file);

    let file = fs::File::create(&config_file)?;
    let writer = BufWriter::new(file);

    serde_json::to_writer_pretty(writer, manager).map_err(|e| {
        error!("VisualCopier: Failed to serialize copier state to {:?}: {}", &config_file, e);
        io::Error::new(io::ErrorKind::Other, e)
    })?;

    Ok(())
}