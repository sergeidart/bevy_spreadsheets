// src/sheets/database/daemon_client.rs
//! Client for SkylineDB daemon - handles all write operations through IPC
//! 
//! Architecture:
//! - All WRITES go through daemon (serialized, no conflicts)
//! - All READS stay direct (maximum performance)
//! - Daemon auto-starts if not running
//! - Uses Windows Named Pipes for IPC

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;

/// Request sent to daemon
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum DaemonRequest {
    /// Execute a batch of SQL statements atomically
    ExecBatch {
        stmts: Vec<Statement>,
        tx: TransactionMode,
    },
    /// Check if daemon is alive
    Ping,
    /// Gracefully shutdown the daemon process
    /// WARNING: This stops the daemon for ALL clients
    Shutdown,
    /// Disconnect this client (daemon continues running)
    Disconnect,
}

/// Single SQL statement with parameters
#[derive(Debug, Serialize)]
pub struct Statement {
    pub sql: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<serde_json::Value>,
}

/// Transaction mode for batch execution
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionMode {
    /// All statements in one atomic transaction (recommended)
    Atomic,
}

/// Protocol version for daemon communication
pub const PROTOCOL_VERSION: u64 = 1;

/// Response from daemon
#[derive(Debug, Deserialize)]
pub struct DaemonResponse {
    pub status: String,
    #[serde(default)]
    pub rev: Option<u64>,
    #[serde(default)]
    pub rows_affected: Option<usize>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Client for communicating with SkylineDB daemon
pub struct DaemonClient {
    pipe_name: String,
    daemon_exe_path: PathBuf,
    #[cfg(test)]
    is_mock: bool,
}

impl DaemonClient {
    /// Create a new daemon client
    /// 
    /// # Arguments
    /// * `pipe_name` - Name of the pipe (default: "SkylineDBd-v1")
    /// * `daemon_exe_path` - Path to daemon executable for auto-start
    pub fn new(pipe_name: Option<&str>, daemon_exe_path: String) -> Self {
        let pipe_name = format!(r"\\.\pipe\{}", pipe_name.unwrap_or("SkylineDBd-v1"));
        
        Self {
            pipe_name,
            daemon_exe_path: PathBuf::from(daemon_exe_path),
            #[cfg(test)]
            is_mock: false,
        }
    }

    /// Create a mock daemon client for testing
    #[cfg(test)]
    pub fn new_mock() -> Self {
        Self {
            pipe_name: String::new(),
            daemon_exe_path: PathBuf::new(),
            is_mock: true,
        }
    }

    /// Send a request to the daemon
    pub fn send_request(&self, request: &DaemonRequest) -> Result<DaemonResponse, String> {
        #[cfg(test)]
        if self.is_mock {
            return Ok(DaemonResponse {
                status: "ok".to_string(),
                rev: Some(PROTOCOL_VERSION),
                rows_affected: Some(1),
                error: None,
            });
        }

        // Try to connect to daemon
        match self.connect_with_retry(3) {
            Ok(mut stream) => self.execute_request(&mut stream, request),
            Err(e) => Err(format!("Failed to connect to daemon: {}", e)),
        }
    }

    /// Connect to daemon with retry logic
    fn connect_with_retry(&self, max_retries: usize) -> Result<Box<dyn ReadWrite>, String> {
        for attempt in 0..max_retries {
            match self.try_connect() {
                Ok(stream) => return Ok(stream),
                Err(_e) if attempt == 0 => {
                    // First attempt failed - try to start daemon
                    bevy::log::info!("Daemon not running, attempting to start it...");
                    if let Err(start_err) = self.start_daemon() {
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
    fn try_connect(&self) -> Result<Box<dyn ReadWrite>, String> {
        use std::fs::OpenOptions;

        match OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.pipe_name)
        {
            Ok(file) => Ok(Box::new(file)),
            Err(e) => Err(format!("Failed to open pipe: {}", e)),
        }
    }

    #[cfg(not(windows))]
    fn try_connect(&self) -> Result<Box<dyn ReadWrite>, String> {
        use std::os::unix::net::UnixStream;
        
        match UnixStream::connect("/tmp/skylinedb-v1.sock") {
            Ok(stream) => Ok(Box::new(stream)),
            Err(e) => Err(format!("Failed to connect to Unix socket: {}", e)),
        }
    }

    /// Start the daemon process
    fn start_daemon(&self) -> Result<(), String> {
        // Check if daemon executable exists
        if !self.daemon_exe_path.exists() {
            return Err(format!(
                "Daemon executable not found at {:?}. Please ensure it's downloaded.",
                self.daemon_exe_path
            ));
        }

        // Find the database file to pass to daemon
        let data_path = crate::sheets::systems::io::get_default_data_base_path();
        let db_file = std::fs::read_dir(&data_path)
            .ok()
            .and_then(|entries| {
                entries
                    .filter_map(Result::ok)
                    .find(|e| e.path().extension().map_or(false, |ext| ext == "db"))
                    .map(|e| e.path())
            })
            .ok_or("No .db file found in data directory")?;

        bevy::log::info!("Starting daemon with executable: {:?}, database: {:?}", self.daemon_exe_path, db_file);
        
        #[cfg(windows)]
        {
            use std::process::Command;
            use std::os::windows::process::CommandExt;
            
            // CREATE_NO_WINDOW flag to prevent console window
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            
            Command::new(&self.daemon_exe_path)
                .arg(&db_file)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("Failed to start daemon: {}", e))?;
        }
        
        #[cfg(not(windows))]
        {
            use std::process::Command;
            
            Command::new(&self.daemon_exe_path)
                .arg(&db_file)
                .spawn()
                .map_err(|e| format!("Failed to start daemon: {}", e))?;
        }
        
        Ok(())
    }

    /// Execute a request on an open connection
    fn execute_request(
        &self,
        stream: &mut Box<dyn ReadWrite>,
        request: &DaemonRequest,
    ) -> Result<DaemonResponse, String> {
        // Serialize request to JSON
        let json = serde_json::to_string(request)
            .map_err(|e| format!("Failed to serialize request: {}", e))?;
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
        
        let response: DaemonResponse = serde_json::from_str(&response_str)
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Validate protocol version
        if let Some(rev) = response.rev {
            if rev != PROTOCOL_VERSION {
                bevy::log::warn!(
                    "Protocol version mismatch: expected {}, got {}",
                    PROTOCOL_VERSION, rev
                );
            }
        }

        if response.status == "error" {
            return Err(response.error.unwrap_or_else(|| "Unknown error".to_string()));
        }

        Ok(response)
    }

    /// Execute a batch of SQL statements atomically
    pub fn exec_batch(&self, statements: Vec<Statement>) -> Result<DaemonResponse, String> {
        let request = DaemonRequest::ExecBatch {
            stmts: statements,
            tx: TransactionMode::Atomic,
        };
        self.send_request(&request)
    }

    /// Disconnect this client from the daemon
    /// The daemon process continues running for other clients
    pub fn disconnect(&self) -> Result<(), String> {
        #[cfg(test)]
        if self.is_mock {
            return Ok(());
        }

        match self.send_request(&DaemonRequest::Disconnect) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to disconnect from daemon: {}", e)),
        }
    }

    /// Shutdown the daemon process
    /// WARNING: This stops the daemon for ALL clients
    /// Use disconnect() if you only want to close this client's connection
    /// 
    /// # When to use
    /// - Administrative shutdown command
    /// - Debug/development tools
    /// - When you're sure no other clients need the daemon
    /// 
    /// The daemon will automatically restart on next write operation if needed
    pub fn shutdown_daemon(&self) -> Result<(), String> {
        #[cfg(test)]
        if self.is_mock {
            return Ok(());
        }

        match self.send_request(&DaemonRequest::Shutdown) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to shutdown daemon: {}", e)),
        }
    }

    /// Check if daemon is running
    pub fn ping(&self) -> bool {
        matches!(self.send_request(&DaemonRequest::Ping), Ok(_))
    }

    /// Execute ALTER TABLE statement through daemon
    /// This adds a column only if it doesn't already exist (idempotent)
    pub fn exec_alter_table(
        &self,
        table_name: &str,
        column_name: &str,
        column_type: &str,
        default_value: &str,
    ) -> Result<(), String> {
        // SQLite's ALTER TABLE will fail with "duplicate column" if column exists
        // We catch this error and treat it as success (idempotent behavior)
        let alter_sql = format!(
            "ALTER TABLE \"{}\" ADD COLUMN {} {} DEFAULT {}",
            table_name, column_name, column_type, default_value
        );
        
        let stmt = Statement {
            sql: alter_sql,
            params: vec![],
        };
        
        match self.exec_batch(vec![stmt]) {
            Ok(_) => Ok(()),
            Err(e) if e.contains("duplicate column") => {
                // Column already exists, this is fine
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Execute INSERT into metadata table through daemon
    pub fn exec_insert_metadata(
        &self,
        meta_table: &str,
        column_index: i32,
        column_name: &str,
        data_type: &str,
    ) -> Result<(), String> {
        let sql = format!(
            "INSERT INTO \"{}\" (column_index, column_name, data_type, validator_type, deleted) \
             VALUES (?, ?, ?, ?, 0)",
            meta_table
        );
        
        let stmt = Statement {
            sql,
            params: vec![
                serde_json::Value::Number(column_index.into()),
                serde_json::Value::String(column_name.to_string()),
                serde_json::Value::String(data_type.to_string()),
                serde_json::Value::String("Basic".to_string()),
            ],
        };
        
        self.exec_batch(vec![stmt])?;
        Ok(())
    }
}

/// Trait for Read + Write to allow platform-specific stream types
trait ReadWrite: Read + Write {}

#[cfg(windows)]
impl ReadWrite for std::fs::File {}

#[cfg(not(windows))]
impl ReadWrite for std::os::unix::net::UnixStream {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_client_lifecycle() {
        // Create a mock client
        let client = DaemonClient::new_mock();
        
        // Test disconnect
        assert!(client.disconnect().is_ok());
        
        // Test shutdown
        assert!(client.shutdown_daemon().is_ok());
        
        // Test ping
        assert!(client.ping());
    }
}
