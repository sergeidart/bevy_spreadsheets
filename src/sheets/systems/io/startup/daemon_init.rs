// src/sheets/systems/io/startup/daemon_init.rs
//! Initialize and ensure SQLite daemon is running for write operations

use bevy::prelude::*;
use crate::sheets::database::daemon_manager;

/// Resource to track daemon initialization state
#[derive(Resource, Default)]
pub struct DaemonState {
    pub is_installed: bool,
    pub is_running: bool,
    pub last_error: Option<String>,
}

/// System that ensures daemon is downloaded and started before any database operations
/// This runs during startup before any database writes occur
pub fn ensure_daemon_ready(
    mut commands: Commands,
    existing_state: Option<Res<DaemonState>>,
) {
    // Skip if already initialized
    if existing_state.is_some() {
        return;
    }

    info!("Checking SQLite daemon status...");

    let mut state = DaemonState::default();

    // Check if daemon is already running (maybe from previous app instance)
    if daemon_manager::is_daemon_running() {
        info!("SQLite daemon is already running");
        state.is_installed = true;
        state.is_running = true;
        commands.insert_resource(state);
        return;
    }

    // Check if daemon executable exists
    state.is_installed = daemon_manager::is_daemon_installed();
    
    if !state.is_installed {
        warn!("SQLite daemon is not installed. Write operations will be queued until daemon is downloaded.");
        warn!("The daemon will be downloaded automatically in the background.");
        
        // Spawn async task to download daemon
        // Note: We don't block startup for this - the daemon_client will handle auto-start attempts
        state.last_error = Some("Daemon not installed - will download on first write attempt".to_string());
    } else {
        info!("SQLite daemon is installed but not running. It will start automatically on first write.");
    }

    commands.insert_resource(state);
}

/// System to initiate daemon download if not installed
pub fn initiate_daemon_download_if_needed(
    state: Res<DaemonState>,
    task_runtime: Res<bevy_tokio_tasks::TokioTasksRuntime>,
) {
    // If daemon is not installed, start downloading it immediately
    if !state.is_installed {
        info!("Starting background download of SQLite daemon...");
        
        // Spawn async task
        task_runtime.spawn_background_task(|_ctx| async move {
            match daemon_manager::ensure_daemon_installed().await {
                Ok(path) => {
                    info!("Successfully downloaded daemon to: {:?}", path);
                }
                Err(e) => {
                    error!("Failed to download daemon: {}", e);
                }
            }
        });
    }
}

/// Warning system that alerts user if daemon is not available
pub fn check_daemon_health(
    state: Res<DaemonState>,
    mut last_warning: Local<Option<std::time::Instant>>,
) {
    // Only warn every 30 seconds
    let now = std::time::Instant::now();
    if let Some(last) = *last_warning {
        if now.duration_since(last).as_secs() < 30 {
            return;
        }
    }

    if !state.is_running && !daemon_manager::is_daemon_running() {
        warn!("⚠️ SQLite daemon is not running. Database writes may fail!");
        warn!("   This means your changes will NOT be saved until the daemon starts.");
        warn!("   Please check that skylinedb-daemon.exe is present in your SkylineDB folder.");
        
        if let Some(ref error) = state.last_error {
            warn!("   Last error: {}", error);
        }
        
        *last_warning = Some(now);
    }
}
