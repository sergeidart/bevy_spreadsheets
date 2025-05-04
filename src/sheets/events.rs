// src/sheets/events.rs
use bevy::prelude::Event;
use std::path::PathBuf;

// Corrected import path
use super::definitions::{SheetGridData, ColumnValidator};

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

#[derive(Event, Debug, Clone)]
pub struct RequestUpdateColumnName {
    pub sheet_name: String,
    pub column_index: usize,
    pub new_name: String,
}

// --- NEW Event for Cell Updates ---
#[derive(Event, Debug, Clone)]
pub struct UpdateCellEvent {
    pub sheet_name: String,
    pub row_index: usize, // Use original row index
    pub col_index: usize,
    pub new_value: String,
}

// --- Event for Validator Updates ---
#[derive(Event, Debug, Clone)]
pub struct RequestUpdateColumnValidator {
    pub sheet_name: String,
    pub column_index: usize,
    pub new_validator: Option<ColumnValidator>, // Use Option to allow clearing/resetting
}