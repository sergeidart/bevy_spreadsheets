// src/sheets/database/daemon_manager.rs
//! Manages the SQLite daemon lifecycle - downloading, installing, and starting

use std::path::PathBuf;
use std::fs;
use std::io::Write;

const DAEMON_RELEASE_URL: &str = "https://github.com/sergeidart/sqlite_daemon/releases/download/V1.2";
const DAEMON_EXE_NAME: &str = "skylinedb-daemon.exe";

/// Get the path where daemon executable should be located
/// Stored in Documents/SkylineDB/daemon/skylinedb-daemon.exe
pub fn get_daemon_path() -> PathBuf {
    crate::sheets::systems::io::get_default_data_base_path()
        .join("daemon")
        .join(DAEMON_EXE_NAME)
}

/// Check if daemon executable exists
pub fn is_daemon_installed() -> bool {
    get_daemon_path().exists()
}

/// Download and install the daemon from GitHub releases
pub async fn download_and_install_daemon() -> Result<PathBuf, String> {
    let daemon_path = get_daemon_path();
    
    // Ensure directory exists
    if let Some(parent) = daemon_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    bevy::log::info!("Downloading daemon from GitHub releases...");
    
    let download_url = format!("{}/{}", DAEMON_RELEASE_URL, DAEMON_EXE_NAME);
    
    // Download the file
    let response = reqwest::get(&download_url)
        .await
        .map_err(|e| format!("Failed to download daemon: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("Failed to download daemon: HTTP {}", response.status()));
    }
    
    let bytes = response.bytes()
        .await
        .map_err(|e| format!("Failed to read daemon binary: {}", e))?;
    
    bevy::log::info!("Downloaded {} bytes, writing to {:?}", bytes.len(), daemon_path);
    
    // Write to file
    let mut file = fs::File::create(&daemon_path)
        .map_err(|e| format!("Failed to create daemon file: {}", e))?;
    
    file.write_all(&bytes)
        .map_err(|e| format!("Failed to write daemon binary: {}", e))?;
    
    bevy::log::info!("Daemon installed successfully at {:?}", daemon_path);
    
    Ok(daemon_path)
}

/// Ensure daemon is installed, downloading if necessary
pub async fn ensure_daemon_installed() -> Result<PathBuf, String> {
    let daemon_path = get_daemon_path();
    
    if daemon_path.exists() {
        bevy::log::debug!("Daemon already installed at {:?}", daemon_path);
        Ok(daemon_path)
    } else {
        bevy::log::info!("Daemon not found, downloading from GitHub...");
        download_and_install_daemon().await
    }
}

/// Check if daemon is currently running by trying to connect
pub fn is_daemon_running() -> bool {
    use super::daemon_client::DaemonClient;
    
    let client = DaemonClient::new(None, get_daemon_path().to_string_lossy().to_string());
    client.ping(None)  // Ping the router daemon without specifying a database
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_daemon_path() {
        let path = get_daemon_path();
        assert!(path.ends_with(DAEMON_EXE_NAME));
    }
}
