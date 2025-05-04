// src/sheets/plugin.rs
use bevy::prelude::*;

use super::resources::SheetRegistry;
use super::events::{
    // Import all necessary events
    AddSheetRowRequest, JsonSheetUploaded, RequestRenameSheet, RequestDeleteSheet,
    RequestDeleteSheetFile, RequestRenameSheetFile, SheetOperationFeedback,
    RequestInitiateFileUpload, RequestProcessUpload, RequestUpdateColumnName,
};

// Adjust IO system imports to reflect the new structure
use super::systems::io::{
    // Import from the NEW startup module
    startup::{
        register_sheet_metadata,
        load_registered_sheets_startup,
        scan_directory_for_sheets_startup, // scan moved here
    },
    // Import runtime upload handlers from load module
    load::{
        handle_json_sheet_upload,
        handle_initiate_file_upload,
        handle_process_upload_request,
    },
    // Import from save module (remains the same)
    save::{
        handle_delete_sheet_file_request,
        handle_rename_sheet_file_request,
        // save_single_sheet is used internally by other systems, not usually registered directly
    },
    // No longer need direct import from scan module
};
// Import logic systems (remain the same)
use super::systems::logic::{
    handle_add_row_request,
    handle_rename_request,
    handle_delete_request,
    handle_update_column_name,
};

// Define system sets for ordering (remain the same)
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum SheetSystemSet {
    UserInput,      // Systems reacting directly to UI events (rename, delete, add row, upload init)
    ApplyChanges,   // Systems processing data and modifying registry (process upload, handle upload result)
    FileOperations, // Systems performing file IO requests (delete/rename file requests from save module)
}


/// Plugin for managing sheet data within the standalone app.
pub struct SheetsPlugin;

impl Plugin for SheetsPlugin {
    fn build(&self, app: &mut App) {
        // Configure system sets for ordering
        app.configure_sets(Update,
            (
                SheetSystemSet::UserInput,
                SheetSystemSet::ApplyChanges,
                SheetSystemSet::FileOperations,
            ).chain() // Ensure strict order between these sets
        );


        // --- Resource Initialization ---
        app.init_resource::<SheetRegistry>();


        // --- Event Registration ---
        // Register all events used by the systems
        app.add_event::<AddSheetRowRequest>()
           .add_event::<JsonSheetUploaded>()
           .add_event::<RequestRenameSheet>()
           .add_event::<RequestDeleteSheet>()
           .add_event::<RequestDeleteSheetFile>()
           .add_event::<RequestRenameSheetFile>()
           .add_event::<SheetOperationFeedback>()
           .add_event::<RequestInitiateFileUpload>()
           .add_event::<RequestProcessUpload>()
           .add_event::<RequestUpdateColumnName>();


        // --- Startup Systems ---
        // Add systems from the new startup.rs module
        app.add_systems(Startup,
            (
                // These now come from systems::io::startup
                register_sheet_metadata,
                apply_deferred, // Apply registry changes from registration before loading
                load_registered_sheets_startup,
                apply_deferred, // Apply registry changes from loading before scanning
                scan_directory_for_sheets_startup,
            ).chain(), // Ensure they run in this specific order at startup
        );

        // --- Update Systems (Organized into Sets) ---

        // Set 1: Handle direct user interactions and logic that modifies the registry
        app.add_systems(Update,
            (
                // Upload initiation (starts the upload workflow)
                handle_initiate_file_upload, // from load.rs

                // Direct registry modifications (trigger saves internally)
                handle_rename_request,       // from logic.rs
                handle_delete_request,       // from logic.rs (triggers file delete event)
                handle_add_row_request,      // from logic.rs
                handle_update_column_name, // from logic.rs
            ).in_set(SheetSystemSet::UserInput)
        );

        // Set 2: Process intermediate steps and apply registry changes from workflows
        app.add_systems(Update,
            (
                 // Process the file path from the upload request
                 handle_process_upload_request, // from load.rs (sends JsonSheetUploaded)
                 apply_deferred, // Ensure JsonSheetUploaded event is available in the same frame
                 // Handle the processed/parsed upload data
                 handle_json_sheet_upload,      // from load.rs (updates registry, triggers save)
            ).chain() // Ensure process runs before handle within this set
             .in_set(SheetSystemSet::ApplyChanges)
        );

        // Set 3: Handle low-level file operations requested by other systems
        app.add_systems(Update,
            (
                // These respond to RequestDeleteSheetFile / RequestRenameSheetFile events
                handle_delete_sheet_file_request, // from save.rs
                handle_rename_sheet_file_request, // from save.rs
            ).in_set(SheetSystemSet::FileOperations)
        );


        info!("SheetsPlugin initialized (using startup.rs and load.rs for IO).");
    }
}