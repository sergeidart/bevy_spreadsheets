// src/sheets/events.rs
use bevy::prelude::Event;
use super::definitions::SheetGridData; // Import SheetGridData

/// Event sent when the user clicks the "Save All Sheets" button in the UI.
/// Handled by systems in `sheets::systems::io`.
#[derive(Event, Debug, Clone)]
pub struct RequestSaveSheets;

/// Event sent when the user clicks the "Add Row" button in the sheet editor UI.
/// Handled by systems in `sheets::systems::logic`.
#[derive(Event, Debug, Clone)]
pub struct AddSheetRowRequest {
    pub sheet_name: String, // Use String
}

/// Event sent when a JSON file has been successfully loaded and parsed by the UI.
/// Handled by a system to add the data to the SheetRegistry.
#[derive(Event, Debug, Clone)]
pub struct JsonSheetUploaded {
    pub desired_sheet_name: String, // Name derived from filename
    pub original_filename: String, // Keep original filename for reference/saving
    pub grid_data: Vec<Vec<String>>, // The parsed grid
}