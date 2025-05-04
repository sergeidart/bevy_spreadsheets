// src/sheets/events.rs
use bevy::prelude::Event;
use std::path::PathBuf;

use super::definitions::SheetGridData; // Keep if SheetGridData used elsewhere

// --- RequestSaveSheets struct removed ---
// #[derive(Event, Debug, Clone)]
// pub struct RequestSaveSheets;

// --- Other events remain ---
#[derive(Event, Debug, Clone)]
pub struct AddSheetRowRequest {
    pub sheet_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct JsonSheetUploaded {
    pub desired_sheet_name: String,
    pub original_filename: String,
    pub grid_data: Vec<Vec<String>>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestRenameSheet {
    pub old_name: String,
    pub new_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteSheet {
    pub sheet_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteSheetFile {
    pub filename: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestRenameSheetFile {
    pub old_filename: String,
    pub new_filename: String,
}

#[derive(Event, Debug, Clone)]
pub struct SheetOperationFeedback {
    pub message: String,
    pub is_error: bool,
}

#[derive(Event, Debug, Clone)]
pub struct RequestInitiateFileUpload;

#[derive(Event, Debug, Clone)]
pub struct RequestProcessUpload {
    pub path: PathBuf,
}