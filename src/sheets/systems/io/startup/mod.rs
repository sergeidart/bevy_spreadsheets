// src/sheets/systems/io/startup/mod.rs

// Declare the modules within the startup directory
pub mod daemon_init;
pub mod grid_load;
pub mod load_registered;
pub mod metadata_load;
pub mod registration;
pub mod scan;
pub mod scan_handlers;

// --- Optional: Re-export key functions for easier access from io/mod.rs ---
// This keeps the public interface exposed via io::startup::* consistent
pub use daemon_init::{ensure_daemon_ready, initiate_daemon_download_if_needed, check_daemon_health};
pub use load_registered::load_data_for_registered_sheets;
pub use registration::register_default_sheets_if_needed;
pub use scan::{scan_and_load_database_files, scan_filesystem_for_unregistered_sheets};
// metadata_load and grid_load helpers are likely internal to the startup process
// and might not need re-exporting unless used elsewhere.
