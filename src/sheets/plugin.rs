// src/sheets/plugin.rs
use bevy::prelude::*;
// Duration import likely not needed here anymore

use super::resources::SheetRegistry;
use super::events::{
    // RequestSaveSheets is removed
    AddSheetRowRequest, JsonSheetUploaded,
    RequestRenameSheet, RequestDeleteSheet, RequestDeleteSheetFile, RequestRenameSheetFile,
    SheetOperationFeedback, RequestInitiateFileUpload, RequestProcessUpload
};

use super::systems::io::{
    register_sheet_metadata,
    load_registered_sheets_startup,
    scan_directory_for_sheets_startup,
    // handle_save_request removed
    handle_json_sheet_upload,
    handle_delete_sheet_file_request,
    handle_rename_sheet_file_request,
    handle_initiate_file_upload,
    handle_process_upload_request,
    // Autosave resources and systems REMOVED
    // save_all_sheets_logic is now just a function, not registered system
};
use super::systems::logic::{
    handle_add_row_request,
    handle_rename_request,
    handle_delete_request,
};

// Define system sets for ordering
// REMOVED Autosave-related sets
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum SheetSystemSet {
    UserInput,      // Systems reacting directly to UI events
    ApplyChanges,   // Systems modifying the registry
    FileOperations, // Systems performing file IO (delete/rename file requests)
}


/// Plugin for managing sheet data within the standalone app.
pub struct SheetsPlugin;

impl Plugin for SheetsPlugin {
    fn build(&self, app: &mut App) {
        // Configure system sets for more granular control
        // REMOVED DetectChanges, AutoSave sets
        app.configure_sets(Update,
            (
                SheetSystemSet::UserInput,
                SheetSystemSet::ApplyChanges,
                SheetSystemSet::FileOperations,
            ).chain() // Ensure strict order between these sets
        );


        // --- Resource Initialization ---
        app.init_resource::<SheetRegistry>();
           // REMOVED: .init_resource::<AutoSaveConfig>();


        // --- Event Registration ---
        // RequestSaveSheets is removed
        app.add_event::<AddSheetRowRequest>()
           .add_event::<JsonSheetUploaded>()
           .add_event::<RequestRenameSheet>()
           .add_event::<RequestDeleteSheet>()
           .add_event::<RequestDeleteSheetFile>()
           .add_event::<RequestRenameSheetFile>()
           .add_event::<SheetOperationFeedback>()
           .add_event::<RequestInitiateFileUpload>()
           .add_event::<RequestProcessUpload>();


        // --- Startup Systems ---
        // REMOVED setup_autosave_state
        app.add_systems(Startup,
            (
                register_sheet_metadata,
                apply_deferred,
                load_registered_sheets_startup,
                apply_deferred,
                scan_directory_for_sheets_startup, // This now calls save internally if needed
                // Removed apply_deferred and setup_autosave_state
            ).chain(),
        );

        // --- Update Systems (Organized into Sets) ---

        app.add_systems(Update,
            (
                // Handles UI request for upload
                handle_initiate_file_upload,
                // Handle other direct UI requests
                handle_rename_request, // Modifies registry, triggers save internally now
                handle_delete_request, // Modifies registry, no save needed
                handle_add_row_request, // Modifies registry, triggers save internally now
            ).in_set(SheetSystemSet::UserInput)
        );

        app.add_systems(Update,
            (
                 // Handles path from RequestInitiateFileUpload, parses, sends next event
                 handle_process_upload_request,
                 apply_deferred, // Keep if needed for event propagation
                 // Handles parsed data, updates registry, sends feedback, triggers save internally now
                 handle_json_sheet_upload,
            ).chain() // Chain systems *within* this set
             .in_set(SheetSystemSet::ApplyChanges)
        );

        // REMOVED: SheetSystemSet::DetectChanges set

        app.add_systems(Update,
            (
                // Handle file operations triggered by ApplyChanges (e.g., delete sheet)
                // handle_save_request removed
                handle_delete_sheet_file_request,
                handle_rename_sheet_file_request,
            ).in_set(SheetSystemSet::FileOperations)
        );

        // REMOVED: SheetSystemSet::AutoSave set


        info!("SheetsPlugin initialized with immediate save-on-change (autosave timer removed).");
    }
}