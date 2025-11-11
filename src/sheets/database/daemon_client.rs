// src/sheets/database/daemon_client.rs
//! Client for SkylineDB daemon - handles all write operations through IPC
//! 
//! Architecture:
//! - All WRITES go through daemon (serialized, no conflicts)
//! - All READS stay direct (maximum performance)
//! - Daemon auto-starts if not running
//! - Uses Windows Named Pipes for IPC

use std::path::PathBuf;

use super::daemon_connection;
// Re-export protocol types for backward compatibility
pub use super::daemon_protocol::{Statement, DaemonRequest, DaemonResponse, TransactionMode};

/// Client for communicating with SkylineDB daemon
pub struct DaemonClient {
    pipe_name: String,
    daemon_exe_path: PathBuf,
    database_name: Option<String>,  // Name of the database file (e.g., "galaxy.db")
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
            database_name: None,
            #[cfg(test)]
            is_mock: false,
        }
    }

    /// Set the database name for this client
    /// Should be the filename only (e.g., "galaxy.db"), not the full path
    /// 
    /// Note: Currently auto-detection is preferred via get_db_name()
    #[allow(dead_code)]
    pub fn set_database(&mut self, db_name: String) {
        self.database_name = Some(db_name);
    }

    /// Get the current database name
    /// 
    /// Note: Currently auto-detection is preferred via get_db_name()
    #[allow(dead_code)]
    pub fn database_name(&self) -> Option<&str> {
        self.database_name.as_deref()
    }

    /// Get database name or extract from path
    fn get_db_name(&self, db_path: Option<&str>) -> Result<String, String> {
        // If explicitly provided, use that
        if let Some(path) = db_path {
            return std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "Invalid database path".to_string());
        }
        
        // Otherwise use the stored database name
        if let Some(db_name) = &self.database_name {
            return Ok(db_name.clone());
        }
        
        // Fallback: try to find any .db file in the data directory
        let data_path = crate::sheets::systems::io::get_default_data_base_path();
        std::fs::read_dir(&data_path)
            .ok()
            .and_then(|entries| {
                entries
                    .filter_map(Result::ok)
                    .find(|e| e.path().extension().map_or(false, |ext| ext == "db"))
                    .and_then(|e| e.file_name().to_str().map(|s| s.to_string()))
            })
            .ok_or_else(|| "No database name set and no .db file found".to_string())
    }

    /// Create a mock daemon client for testing
    #[cfg(test)]
    pub fn new_mock() -> Self {
        Self {
            pipe_name: String::new(),
            daemon_exe_path: PathBuf::new(),
            database_name: Some("test.db".to_string()),
            is_mock: true,
        }
    }

    /// Send a request to the daemon
    pub fn send_request(&self, request: &DaemonRequest) -> Result<DaemonResponse, String> {
        #[cfg(test)]
        if self.is_mock {
            use super::daemon_protocol::PROTOCOL_VERSION;
            return Ok(DaemonResponse {
                status: "ok".to_string(),
                rev: Some(PROTOCOL_VERSION),
                rows_affected: Some(1),
                error: None,
                message: None,
                code: None,
                checkpointed: None,
                closed: None,
                reopened: None,
            });
        }

        // Try to connect to daemon
        match daemon_connection::connect_with_retry(&self.pipe_name, &self.daemon_exe_path, 3) {
            Ok(mut stream) => daemon_connection::execute_request(&mut stream, request),
            Err(e) => Err(format!("Failed to connect to daemon: {}", e)),
        }
    }

    /// Execute a batch of SQL statements atomically
    /// 
    /// # Arguments
    /// * `statements` - Vector of SQL statements to execute
    /// * `db_path` - Optional database path. If None, uses the stored database name
    pub fn exec_batch(&self, statements: Vec<Statement>, db_path: Option<&str>) -> Result<DaemonResponse, String> {
        let db_name = self.get_db_name(db_path)?;
        
        let request = DaemonRequest::ExecBatch {
            db: db_name,
            stmts: statements,
            tx: TransactionMode::Atomic,
        };
        self.send_request(&request)
    }

    /// Prepare database for maintenance (checkpoint WAL)
    /// 
    /// This ensures all changes from the WAL file are written to the main database file.
    /// Use this before operations that need to work with the database file (like rename or backup).
    /// 
    /// # Arguments
    /// * `db_path` - Optional database path. If None, uses the stored database name
    pub fn prepare_for_maintenance(&self, db_path: Option<&str>) -> Result<DaemonResponse, String> {
        let db_name = self.get_db_name(db_path)?;
        
        let request = DaemonRequest::PrepareForMaintenance {
            db: db_name,
        };
        self.send_request(&request)
    }

    /// Close database (release file locks for replacement)
    /// 
    /// Closes the database connection in the daemon, releasing all file locks.
    /// This allows file operations like rename, delete, or replace to succeed.
    /// 
    /// IMPORTANT: After closing, you must call reopen_database() before any database operations.
    /// 
    /// # Arguments
    /// * `db_path` - Optional database path. If None, uses the stored database name
    pub fn close_database(&self, db_path: Option<&str>) -> Result<DaemonResponse, String> {
        let db_name = self.get_db_name(db_path)?;
        
        let request = DaemonRequest::CloseDatabase {
            db: db_name,
        };
        self.send_request(&request)
    }

    /// Reopen database after maintenance
    /// 
    /// Reopens the database connection in the daemon after a close_database() call.
    /// The database file must exist at its expected location.
    /// 
    /// # Arguments
    /// * `db_path` - Optional database path. If None, uses the stored database name
    pub fn reopen_database(&self, db_path: Option<&str>) -> Result<DaemonResponse, String> {
        let db_name = self.get_db_name(db_path)?;
        
        let request = DaemonRequest::ReopenDatabase {
            db: db_name,
        };
        self.send_request(&request)
    }
    
    /// Perform a safe file operation on the database
    /// 
    /// This helper method properly prepares the database for file operations:
    /// 1. Checkpoints the WAL to ensure all data is in the main file
    /// 2. Closes the database to release file locks
    /// 3. Executes your file operation callback
    /// 4. Reopens the database (with new name if renamed)
    /// 
    /// # Arguments
    /// * `db_path` - Database filename (e.g., "galaxy.db")
    /// * `operation` - Callback that performs the file operation
    /// * `new_db_name` - Optional new database name if the operation renames the file
    /// 
    /// # Example
    /// ```no_run
    /// # use std::fs;
    /// # let client = daemon_client;
    /// # let old_path = std::path::Path::new("old.db");
    /// # let new_path = std::path::Path::new("new.db");
    /// client.with_safe_file_operation(
    ///     Some("old.db"),
    ///     || fs::rename(old_path, new_path),
    ///     Some("new.db")
    /// )?;
    /// ```
    pub fn with_safe_file_operation<F, R>(
        &self,
        db_path: Option<&str>,
        operation: F,
        new_db_name: Option<&str>,
    ) -> Result<R, String>
    where
        F: FnOnce() -> Result<R, std::io::Error>,
    {
        let db_name = self.get_db_name(db_path)?;
        
        // Step 1: Checkpoint WAL
        self.prepare_for_maintenance(Some(&db_name))
            .map_err(|e| format!("Failed to checkpoint database: {}", e))?;
        
        // Step 2: Close database
        self.close_database(Some(&db_name))
            .map_err(|e| format!("Failed to close database: {}", e))?;
        
        // Small delay to ensure locks are fully released
        std::thread::sleep(std::time::Duration::from_millis(100));
        
        // Step 3: Perform file operation
        let result = operation()
            .map_err(|e| format!("File operation failed: {}", e))?;
        
        // Step 4: Reopen database (with new name if provided)
        let reopen_name = new_db_name.unwrap_or(&db_name);
        self.reopen_database(Some(reopen_name))
            .map_err(|e| format!("Failed to reopen database: {}", e))?;
        
        Ok(result)
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
    /// 
    /// # Arguments
    /// * `db_name` - Optional database name to check. Provide a database name for a proper health check.
    pub fn ping(&self, db_name: Option<&str>) -> bool {
        // If no database specified, use a placeholder since daemon requires it
        let db = db_name.or(Some("_health_check.db")).map(|s| s.to_string());
        
        let request = DaemonRequest::Ping { db };
        matches!(self.send_request(&request), Ok(_))
    }

    /// Execute ALTER TABLE statement through daemon
    /// This adds a column only if it doesn't already exist (idempotent)
    pub fn exec_alter_table(
        &self,
        table_name: &str,
        column_name: &str,
        column_type: &str,
        default_value: &str,
        db_path: Option<&str>,
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
        
        match self.exec_batch(vec![stmt], db_path) {
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
        db_path: Option<&str>,
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
        
        self.exec_batch(vec![stmt], db_path)?;
        Ok(())
    }
}

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
        assert!(client.ping(None));
    }
}
