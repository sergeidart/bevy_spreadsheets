// src/sheets/plugin.rs
use bevy::prelude::*;

use super::resources::SheetRegistry;
use super::events::{AddSheetRowRequest, RequestSaveSheets, JsonSheetUploaded};

// --- Import systems using their new paths via io/mod.rs re-exports ---
use super::systems::io::{
    register_sheet_metadata,
    load_registered_sheets_startup,
    scan_directory_for_sheets_startup,
    handle_save_request,
    handle_json_sheet_upload,
};
// --- Import logic systems ---
use super::systems::logic::handle_add_row_request;

/// Plugin for managing sheet data within the standalone app.
pub struct SheetsPlugin;

impl Plugin for SheetsPlugin {
    fn build(&self, app: &mut App) {
        // --- Resource Initialization ---
        app.init_resource::<SheetRegistry>();

        // --- Event Registration ---
        app.add_event::<RequestSaveSheets>()
           .add_event::<AddSheetRowRequest>()
           .add_event::<JsonSheetUploaded>();

        // --- Startup Systems ---
        // Ensure strict order: Register -> Load Registered -> Scan Directory
        app.add_systems(
            Startup,
            (
                register_sheet_metadata,
                // Ensure registration completes before load
                apply_deferred,
                // Load sheets based on registration
                load_registered_sheets_startup,
                // Ensure loading completes before scanning
                apply_deferred,
                // Scan directory for any other sheets
                scan_directory_for_sheets_startup,
            )
                .chain(), // Chain ensures strict order
        );

        // --- Update Systems ---
        app.add_systems(
            Update,
            (
                handle_save_request,      // Handles event from UI to save all
                handle_add_row_request,   // Handles event from UI to add row
                handle_json_sheet_upload, // Handles event from UI upload
            ),
        );

        info!("SheetsPlugin initialized with split IO systems.");
    }
}