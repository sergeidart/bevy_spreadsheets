// src/ui/elements/editor/prefs.rs
use bevy::log::{error, info, warn};
use directories_next::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "BevyAppOrg";
const APPLICATION: &str = "BevySpreadsheetEditor";
const CONFIG_FILE: &str = "ui_prefs.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UiPrefs {
    #[serde(default)]
    pub category_picker_expanded: bool,
    #[serde(default)]
    pub sheet_picker_expanded: bool,
    #[serde(default)]
    pub ai_groups_expanded: bool,
}

fn get_prefs_path() -> io::Result<PathBuf> {
    if let Some(proj) = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION) {
        let cfg_dir = proj.config_dir();
        fs::create_dir_all(cfg_dir)?;
        Ok(cfg_dir.join(CONFIG_FILE))
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No project dirs for UI prefs",
        ))
    }
}

pub fn load_prefs() -> UiPrefs {
    match get_prefs_path().and_then(|p| fs::File::open(&p).map(|f| (p, f))) {
        Ok((path, file)) => match serde_json::from_reader::<_, UiPrefs>(file) {
            Ok(prefs) => {
                info!("UI Prefs loaded from {:?}", path);
                // Ensure defaults for new fields
                // (serde default already applied via #[serde(default)])
                prefs
            }
            Err(e) => {
                error!("Failed to parse UI prefs: {}. Using defaults.", e);
                UiPrefs::default()
            }
        },
        Err(e) => {
            warn!("UI Prefs not found or unreadable ({}). Using defaults.", e);
            UiPrefs::default()
        }
    }
}

pub fn save_prefs(prefs: &UiPrefs) {
    match get_prefs_path() {
        Ok(path) => match fs::File::create(&path) {
            Ok(file) => {
                if let Err(e) = serde_json::to_writer_pretty(file, prefs) {
                    error!("Failed to save UI prefs to {:?}: {}", path, e);
                } else {
                    info!("Saved UI prefs to {:?}", path);
                }
            }
            Err(e) => error!("Failed to create UI prefs file: {}", e),
        },
        Err(e) => error!("Failed to get UI prefs path: {}", e),
    }
}
