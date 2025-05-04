// src/sheets/plugin.rs
use bevy::prelude::*;

use super::resources::SheetRegistry;
use super::events::{
    // Import all necessary events
    AddSheetRowRequest, JsonSheetUploaded, RequestRenameSheet, RequestDeleteSheet,
    RequestDeleteSheetFile, RequestRenameSheetFile, SheetOperationFeedback,
    RequestInitiateFileUpload, RequestProcessUpload, RequestUpdateColumnName,
    RequestUpdateColumnValidator,
    UpdateCellEvent,
};

// IO systems (imports remain the same)
use super::systems::io::{
    startup_load::{register_sheet_metadata, load_registered_sheets_startup},
    startup_scan::{scan_directory_for_sheets_startup},
    load::{handle_json_sheet_upload, handle_initiate_file_upload, handle_process_upload_request},
    save::{handle_delete_sheet_file_request, handle_rename_sheet_file_request},
};

// Logic systems (import handlers from new specific modules)
use super::systems::logic::{
    add_row::handle_add_row_request,             // <-- UPDATED path
    rename_sheet::handle_rename_request,         // <-- UPDATED path
    delete_sheet::handle_delete_request,         // <-- UPDATED path
    update_column_name::handle_update_column_name, // <-- UPDATED path
    update_column_validator::handle_update_column_validator, // <-- UPDATED path
    update_cell::handle_cell_update,             // <-- UPDATED path
};

// Define system sets for ordering
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum SheetSystemSet {
    UserInput,      // Systems reacting directly to UI events
    ApplyChanges,   // Systems processing data and modifying registry
    FileOperations, // Systems performing file IO requests
}


/// Plugin for managing sheet data within the standalone app.
pub struct SheetsPlugin;

impl Plugin for SheetsPlugin {
    fn build(&self, app: &mut App) {
        // Configure system sets for ordering
        app.configure_sets(Update,
            (
                SheetSystemSet::UserInput,
                SheetSystemSet::ApplyChanges.after(SheetSystemSet::UserInput),
                SheetSystemSet::FileOperations.after(SheetSystemSet::ApplyChanges),
            )
        );

        // --- Resource Initialization ---
        app.init_resource::<SheetRegistry>();

        // --- Event Registration ---
        app.add_event::<AddSheetRowRequest>()
           .add_event::<JsonSheetUploaded>()
           .add_event::<RequestRenameSheet>()
           .add_event::<RequestDeleteSheet>()
           .add_event::<RequestDeleteSheetFile>()
           .add_event::<RequestRenameSheetFile>()
           .add_event::<SheetOperationFeedback>()
           .add_event::<RequestInitiateFileUpload>()
           .add_event::<RequestProcessUpload>()
           .add_event::<RequestUpdateColumnName>()
           .add_event::<RequestUpdateColumnValidator>()
           .add_event::<UpdateCellEvent>();

        // --- Startup Systems ---
        app.add_systems(Startup,
            (
                register_sheet_metadata,
                apply_deferred,
                load_registered_sheets_startup,
                apply_deferred,
                scan_directory_for_sheets_startup,
            ).chain(),
        );

        // --- Update Systems (Organized into Sets) ---

        // UserInput: Reacts to UI interactions that might generate other events
        app.add_systems(Update,
            (
                handle_initiate_file_upload,
                // handle_process_upload_request needs registry access but primarily reacts to user action result
                // Let's keep it in ApplyChanges for now as it leads to registry modification via JsonSheetUploaded
            ).in_set(SheetSystemSet::UserInput)
        );

        // ApplyChanges: Handles events that modify the registry state
        app.add_systems(Update,
            (
                 // Process uploads first as they add/modify registry state
                 handle_process_upload_request, // Reads registry, writes JsonSheetUploaded
                 apply_deferred, // Ensure JsonSheetUploaded event is available for next system
                 handle_json_sheet_upload, // Reads JsonSheetUploaded, modifies registry, triggers save
                 apply_deferred, // Ensure registry changes are visible for logic handlers

                 // --- Logic handlers triggered by events (using updated imports) ---
                 // Order might matter: Rename/Delete before Add/Update on potentially affected sheets
                 handle_rename_request,
                 handle_delete_request,
                 // Add/Update can likely run in parallel if needed, or keep sequential
                 handle_add_row_request,
                 handle_update_column_name,
                 handle_update_column_validator,
                 handle_cell_update, // Handles individual cell edits

                 // Apply deferred after logic handlers ensure changes are visible for FileOperations
                 apply_deferred,
            )
             // Keep these sequential for now unless performance requires parallelization
             .chain() // <-- Explicitly chain within ApplyChanges
             .in_set(SheetSystemSet::ApplyChanges)
        );

        // FileOperations: Handles file IO based on events from ApplyChanges stage
        app.add_systems(Update,
            (
                // These can likely run in parallel
                handle_delete_sheet_file_request,
                handle_rename_sheet_file_request,
            ).in_set(SheetSystemSet::FileOperations)
        );

        info!("SheetsPlugin initialized (with split logic handlers).");
    }
}