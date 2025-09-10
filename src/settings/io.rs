use directories_next::ProjectDirs;
use std::fs;
use std::io::{self, BufReader, BufWriter, ErrorKind};
use std::path::PathBuf;
use bevy::log::{info, error, debug};

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "BevyAppOrg";
const APPLICATION: &str = "BevySpreadsheetsApp";
const CONFIG_FILE: &str = "app_settings.json";

fn get_config_path() -> io::Result<PathBuf> {
    if let Some(proj_dirs) = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION) {
        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir)?;
        Ok(config_dir.join(CONFIG_FILE))
    } else {
        Err(io::Error::new(io::ErrorKind::NotFound, "Could not determine project directories for app settings."))
    }
}

pub fn load_settings_from_file<T: for<'de> serde::de::Deserialize<'de> + Default>() -> io::Result<T> {
    let config_file = get_config_path()?;
    info!("AppSettings: Attempting to load settings from {:?}", config_file);
    match fs::File::open(&config_file) {
        Ok(file) => {
            let reader = BufReader::new(file);
            match serde_json::from_reader(reader) {
                Ok(settings) => {
                    info!("AppSettings: Successfully deserialized settings.");
                    Ok(settings)
                }
                Err(e) => {
                    error!("AppSettings: Failed to parse settings file {:?}: {}", &config_file, e);
                    Err(io::Error::new(ErrorKind::InvalidData, format!("Failed to parse settings file: {}", e)))
                }
            }
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            info!("AppSettings: Settings file not found at {:?}. Returning default.", config_file);
            Ok(Default::default())
        }
        Err(e) => {
            error!("AppSettings: Failed to open settings file {:?}: {}", &config_file, e);
            Err(e)
        }
    }
}

pub fn save_settings_to_file<T: serde::Serialize>(settings: &T) -> io::Result<()> {
    let config_file = get_config_path()?;
    info!("AppSettings: Saving settings to {:?}", config_file);
    debug!("AppSettings [SAVE_DEBUG]: writing settings");
    let file = fs::File::create(&config_file)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, settings).map_err(|e| {
        error!("AppSettings: Failed to serialize settings to {:?}: {}", &config_file, e);
        io::Error::new(io::ErrorKind::Other, e)
    })?;
    Ok(())
}
