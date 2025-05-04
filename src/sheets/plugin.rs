// src/sheets/plugin.rs
use bevy::prelude::*;

use super::resources::SheetRegistry;
use super::events::{AddSheetRowRequest, RequestSaveSheets};
// Import systems needed by the plugin
use super::systems::io::{
    load_all_sheets_startup, register_sheet_metadata, handle_save_request,
    // get_default_data_base_path, // Not directly used in plugin build
    // save_all_sheets_logic, // Not directly used in plugin build
};
use super::systems::logic::handle_add_row_request;

/// Plugin for managing sheet data within the standalone app.
pub struct SheetsPlugin;

impl Plugin for SheetsPlugin {
    fn build(&self, app: &mut App) {
        // --- Resource Initialization ---
        app.init_resource::<SheetRegistry>();

        // --- Event Registration ---
        app.add_event::<RequestSaveSheets>()
           .add_event::<AddSheetRowRequest>();

        // --- Startup Systems ---
        // System order is important here: register -> load
        app.add_systems(
            Startup,
            (
                // Use the local registration system from io.rs
                register_sheet_metadata,
                // apply_deferred ensures registration completes before load reads the registry
                apply_deferred,
                load_all_sheets_startup,
            ).chain(), // Chain ensures strict order
        );

        // --- Update Systems ---
        app.add_systems(
            Update,
            (
                handle_save_request,    // Handles event from UI to save all
                handle_add_row_request, // Handles event from UI to add row
            ),
        );

        info!("App SheetsPlugin initialized.");
    }
}