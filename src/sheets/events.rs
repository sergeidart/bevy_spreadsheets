// src/sheets/events.rs
use bevy::prelude::Event;

/// Event sent when the user clicks the "Save All Sheets" button in the UI.
/// Handled by systems in `sheets::systems::io`.
#[derive(Event, Debug, Clone)]
pub struct RequestSaveSheets;

/// Event sent when the user clicks the "Add Row" button in the sheet editor UI.
/// Handled by systems in `sheets::systems::logic`.
#[derive(Event, Debug, Clone)]
pub struct AddSheetRowRequest {
    pub sheet_name: &'static str,
}

// Add other sheet-related events here if needed
// e.g., pub struct RequestDeleteRow { sheet_name: &'static str, row_index: usize }