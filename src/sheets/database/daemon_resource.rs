// src/sheets/database/daemon_resource.rs
//! Resource for managing shared daemon client across systems

use bevy::prelude::*;
use super::daemon_client::DaemonClient;
use super::daemon_manager;

/// Event to request daemon shutdown (administrative operation)
#[derive(Event)]
pub struct RequestDaemonShutdown {
    /// Optional reason for shutdown (for logging)
    pub reason: Option<String>,
}

/// Shared daemon client resource
#[derive(Resource)]
pub struct SharedDaemonClient {
    client: DaemonClient,
}

impl SharedDaemonClient {
    /// Create a new shared daemon client
    pub fn new() -> Self {
        let daemon_path = daemon_manager::get_daemon_path();
        let client = DaemonClient::new(
            None, // Use default pipe name
            daemon_path.to_string_lossy().to_string(), // Required exe path
        );
        
        Self { client }
    }
    
    /// Get reference to the daemon client
    pub fn client(&self) -> &DaemonClient {
        &self.client
    }
}

impl Default for SharedDaemonClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Gracefully disconnect from daemon on app exit
/// This ensures clean shutdown and allows the daemon to release resources
pub fn disconnect_on_exit(
    mut exit_reader: EventReader<AppExit>,
    daemon_client: Res<SharedDaemonClient>,
) {
    for _event in exit_reader.read() {
        info!("App exit detected, disconnecting from daemon...");
        
        match daemon_client.client().disconnect() {
            Ok(_) => {
                info!("Successfully disconnected from daemon");
            }
            Err(e) => {
                // Log but don't fail the exit
                warn!("Failed to disconnect from daemon: {}", e);
            }
        }
        
        // Only process the first exit event
        break;
    }
}

/// Handle administrative daemon shutdown requests
/// This system processes RequestDaemonShutdown events and stops the daemon
pub fn handle_daemon_shutdown_request(
    mut shutdown_reader: EventReader<RequestDaemonShutdown>,
    daemon_client: Res<SharedDaemonClient>,
) {
    for event in shutdown_reader.read() {
        let reason = event.reason.as_deref().unwrap_or("Manual shutdown requested");
        info!("Daemon shutdown requested: {}", reason);
        
        match daemon_client.client().shutdown_daemon() {
            Ok(_) => {
                info!("Daemon shutdown successful. Daemon will auto-restart on next write operation.");
            }
            Err(e) => {
                error!("Failed to shutdown daemon: {}", e);
            }
        }
        
        // Only process the first shutdown request
        break;
    }
}
