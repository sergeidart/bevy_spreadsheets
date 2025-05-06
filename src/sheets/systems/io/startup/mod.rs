// src/sheets/systems/io/startup/mod.rs

// Declare the modules within the startup directory
pub mod registration;
pub mod metadata_load;
pub mod grid_load;
pub mod scan;
pub mod load_registered;

// --- Optional: Re-export key functions for easier access from io/mod.rs ---
// This keeps the public interface exposed via io::startup::* consistent
pub use registration::register_default_sheets_if_needed;
pub use load_registered::load_data_for_registered_sheets;
pub use scan::scan_filesystem_for_unregistered_sheets;
// metadata_load and grid_load helpers are likely internal to the startup process
// and might not need re-exporting unless used elsewhere.
