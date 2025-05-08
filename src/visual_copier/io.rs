// src/visual_copier/io.rs

use super::resources::VisualCopierManager;
use directories_next::ProjectDirs;
use std::fs;
use std::io::{self, BufReader, BufWriter, ErrorKind};
use std::path::PathBuf;
// --- MODIFIED: Use bevy::log::* ---
use bevy::log::{debug, error, info};
// --- END MODIFIED ---

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "BevyAppOrg";
const APPLICATION: &str = "BevyVisualCopierApp";
const CONFIG_FILE: &str = "visual_copier_manager_state.json";

/// Helper function to get the configuration file path.
fn get_config_path() -> io::Result<PathBuf> {
    if let Some(proj_dirs) = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION) {
        let config_dir = proj_dirs.config_dir();
        // Ensure the config directory exists before attempting to use it
        fs::create_dir_all(config_dir)?;
        // if let Err(e) = fs::create_dir_all(config_dir) {
        //     error!("VisualCopier: Failed to create config directory {:?}: {}", config_dir, e);
        //     // Convert the error or handle it appropriately - maybe return it?
        //     // For now, just log and continue, hoping the path is usable.
        // }
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
/// Returns an error if the file cannot be read or parsed.
pub fn load_copier_manager_from_file() -> io::Result<VisualCopierManager> {
    let config_file = get_config_path()?;

    info!("VisualCopier: Attempting to load copier state from {:?}", config_file);
    match fs::File::open(&config_file) {
        Ok(file) => {
            let reader = BufReader::new(file);
            match serde_json::from_reader(reader) {
                Ok(manager) => {
                    info!("VisualCopier: Successfully deserialized state.");
                    Ok(manager)
                },
                Err(e) => {
                    error!("VisualCopier: Failed to parse config file {:?}: {}", &config_file, e);
                    Err(io::Error::new(ErrorKind::InvalidData, format!("Failed to parse config file: {}", e)))
                }
            }
        },
        Err(e) if e.kind() == ErrorKind::NotFound => {
             info!("VisualCopier: Config file not found at {:?}. Returning default state.", config_file);
             Ok(VisualCopierManager::default())
        },
        Err(e) => {
             error!("VisualCopier: Failed to open config file {:?}: {}", &config_file, e);
             Err(e)
        }
    }
}

/// Saves the VisualCopierManager state to the configuration file.
pub fn save_copier_manager_to_file(manager: &VisualCopierManager) -> io::Result<()> {
    let config_file = get_config_path()?;
    info!("VisualCopier: Saving {} tasks and top panel paths (CopyOnExit={}) to {:?}",
          manager.copy_tasks.len(), manager.copy_top_panel_on_exit, &config_file);

    // --- RECOMMENDED DEBUG LOG ---
    debug!("VisualCopier [SAVE_DEBUG_IO]: copy_top_panel_on_exit value at serialization time: {}", manager.copy_top_panel_on_exit);
    // --- END DEBUG LOG ---

    let file = fs::File::create(&config_file)?;
    let writer = BufWriter::new(file);

    serde_json::to_writer_pretty(writer, manager).map_err(|e| {
        error!("VisualCopier: Failed to serialize copier state to {:?}: {}", &config_file, e);
        io::Error::new(io::ErrorKind::Other, e)
    })?;

    Ok(())
}