// src/sheets/database/daemon_connection.rs
//! Low-level connection management for daemon communication
//!
//! This module handles:
//! - Opening pipe/socket connections
//! - Auto-starting the daemon process
//! - Retry logic with exponential backoff
//! - Request/response serialization over the wire

use std::io::{Read, Write};
use std::path::Path;

use super::daemon_protocol::{DaemonRequest, DaemonResponse};

/// Trait for Read + Write to allow platform-specific stream types
pub trait ReadWrite: Read + Write {}

#[cfg(windows)]
impl ReadWrite for std::fs::File {}

#[cfg(not(windows))]
impl ReadWrite for std::os::unix::net::UnixStream {}

/// Connect to daemon with retry logic and auto-start
pub fn connect_with_retry(
    pipe_name: &str,
    daemon_exe_path: &Path,
    max_retries: usize,
) -> Result<Box<dyn ReadWrite>, String> {
    for attempt in 0..max_retries {
        match try_connect(pipe_name) {
            Ok(stream) => return Ok(stream),
            Err(_e) if attempt == 0 => {
                // First attempt failed - try to start daemon
                bevy::log::info!("Daemon not running, attempting to start it...");
                if let Err(start_err) = start_daemon(daemon_exe_path) {
                    bevy::log::warn!("Failed to start daemon: {}", start_err);
                }
                // Wait a bit for daemon to start
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(_e) if attempt < max_retries - 1 => {
                bevy::log::debug!("Connection attempt {} failed, retrying...", attempt + 1);
                std::thread::sleep(std::time::Duration::from_millis(200 * (attempt as u64 + 1)));
            }
            Err(e) => return Err(e),
        }
    }
    Err("Max retries exceeded".to_string())
}

/// Try to connect to the daemon pipe
#[cfg(windows)]
fn try_connect(pipe_name: &str) -> Result<Box<dyn ReadWrite>, String> {
    use std::fs::OpenOptions;

    match OpenOptions::new()
        .read(true)
        .write(true)
        .open(pipe_name)
    {
        Ok(file) => Ok(Box::new(file)),
        Err(e) => Err(format!("Failed to open pipe: {}", e)),
    }
}

#[cfg(not(windows))]
fn try_connect(_pipe_name: &str) -> Result<Box<dyn ReadWrite>, String> {
    use std::os::unix::net::UnixStream;
    
    match UnixStream::connect("/tmp/skylinedb-v1.sock") {
        Ok(stream) => Ok(Box::new(stream)),
        Err(e) => Err(format!("Failed to connect to Unix socket: {}", e)),
    }
}

/// Start the daemon process
fn start_daemon(daemon_exe_path: &Path) -> Result<(), String> {
    // Check if daemon executable exists
    if !daemon_exe_path.exists() {
        return Err(format!(
            "Daemon executable not found at {:?}. Please ensure it's downloaded.",
            daemon_exe_path
        ));
    }

    // Get the data directory (daemon manages all .db files in this directory)
    let data_path = crate::sheets::systems::io::get_default_data_base_path();

    bevy::log::info!("Starting daemon with executable: {:?}, data directory: {:?}", daemon_exe_path, data_path);
    
    #[cfg(windows)]
    {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        
        // CREATE_NO_WINDOW flag to prevent console window
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        Command::new(daemon_exe_path)
            .arg(&data_path)  // Pass data directory, not specific database
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to start daemon: {}", e))?;
    }
    
    #[cfg(not(windows))]
    {
        use std::process::Command;
        
        Command::new(daemon_exe_path)
            .arg(&data_path)  // Pass data directory, not specific database
            .spawn()
            .map_err(|e| format!("Failed to start daemon: {}", e))?;
    }
    
    Ok(())
}

/// Execute a request on an open connection
pub fn execute_request(
    stream: &mut Box<dyn ReadWrite>,
    request: &DaemonRequest,
) -> Result<DaemonResponse, String> {
    // Serialize request to JSON
    let json = serde_json::to_string(request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?;
    
    // Debug log the request being sent (truncated for large payloads)
    if json.len() > 500 {
        bevy::log::trace!("Sending daemon request: {}...", &json[..500]);
    } else {
        bevy::log::trace!("Sending daemon request: {}", json);
    }
    
    let json_bytes = json.as_bytes();
    let length = json_bytes.len() as u32;

    // Send length prefix (little-endian u32)
    let length_bytes = length.to_le_bytes();
    stream.write_all(&length_bytes)
        .map_err(|e| format!("Failed to write length: {}", e))?;

    // Send JSON payload
    stream.write_all(json_bytes)
        .map_err(|e| format!("Failed to write JSON: {}", e))?;

    stream.flush()
        .map_err(|e| format!("Failed to flush: {}", e))?;

    // Read response length
    let mut length_buf = [0u8; 4];
    stream.read_exact(&mut length_buf)
        .map_err(|e| format!("Failed to read response length: {}", e))?;
    let response_length = u32::from_le_bytes(length_buf);

    // Read response JSON
    let mut response_buf = vec![0u8; response_length as usize];
    stream.read_exact(&mut response_buf)
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Parse response
    let response_str = String::from_utf8(response_buf)
        .map_err(|e| format!("Invalid UTF-8 in response: {}", e))?;
    
    // Debug log the raw response
    bevy::log::debug!("Raw daemon response: {}", response_str);
    
    let response: DaemonResponse = serde_json::from_str(&response_str)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // NOTE: The 'rev' field in responses is a request counter, not a protocol version
    // It increments with each request: 1, 2, 3, etc.
    // We validate protocol compatibility at connection time instead.

    if response.status == "error" {
        // The daemon may use either 'error' or 'message' field for error text
        let error_msg = response.error
            .or(response.message.clone())
            .unwrap_or_else(|| "Unknown error".to_string());
        
        // During startup, ALTER TABLE commands may be sent to tables that haven't been
        // committed from WAL yet. These "no such table" errors for *_Metadata tables
        // are expected and non-fatal, so we log them at debug level instead of error.
        // We check the full response string since the table name appears in the "message" field.
        let is_metadata_error = response_str.contains("_Metadata");
        let is_no_such_table = response_str.contains("no such table");
        let is_expected_metadata_error = is_metadata_error && is_no_such_table;
        
        // Duplicate column errors are also expected during metadata migrations when
        // freshly-created tables already have the latest schema but migration code
        // tries to add columns anyway. These are harmless and should be logged at debug level.
        let is_duplicate_column = response_str.contains("duplicate column name");
        
        if is_expected_metadata_error {
            bevy::log::debug!("Daemon could not find metadata table (WAL visibility issue - will retry later): {}", error_msg);
        } else if is_duplicate_column {
            bevy::log::debug!("Daemon reported duplicate column (expected during migrations): {}", error_msg);
        } else {
            bevy::log::error!("Daemon returned error: {}. Full response: {}", error_msg, response_str);
        }
        
        return Err(error_msg);
    }

    Ok(response)
}
