// src/sheets/events.rs
use bevy::prelude::Event;
use std::path::PathBuf;

// Corrected import path
use super::definitions::{SheetGridData, ColumnValidator};

#[derive(Event, Debug, Clone)]
pub struct AddSheetRowRequest {
    pub category: Option<String>, // <<< ADDED category
    pub sheet_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct JsonSheetUploaded {
    pub category: Option<String>, // <<< ADDED category (e.g., None for direct uploads)
    pub desired_sheet_name: String,
    pub original_filename: String, // Just the filename part
    pub grid_data: Vec<Vec<String>>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestRenameSheet {
    pub category: Option<String>, // <<< ADDED category
    pub old_name: String,
    pub new_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteSheet {
    pub category: Option<String>, // <<< ADDED category
    pub sheet_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteSheetFile {
    pub relative_path: PathBuf, // <<< CHANGED to relative PathBuf
}

#[derive(Event, Debug, Clone)]
pub struct RequestRenameSheetFile {
    pub old_relative_path: PathBuf, // <<< CHANGED to relative PathBuf
    pub new_relative_path: PathBuf, // <<< CHANGED to relative PathBuf
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
    pub path: PathBuf, // Full path to the uploaded file
    // Consider adding Option<String> category here if upload UI allows category selection
}

#[derive(Event, Debug, Clone)]
pub struct RequestUpdateColumnName {
    pub category: Option<String>, // <<< ADDED category
    pub sheet_name: String,
    pub column_index: usize,
    pub new_name: String,
}

// --- NEW Event for Cell Updates ---
#[derive(Event, Debug, Clone)]
pub struct UpdateCellEvent {
    pub category: Option<String>, // <<< ADDED category
    pub sheet_name: String,
    pub row_index: usize, // Use original row index
    pub col_index: usize,
    pub new_value: String,
}

// --- Event for Validator Updates ---
#[derive(Event, Debug, Clone)]
pub struct RequestUpdateColumnValidator {
    pub category: Option<String>, // <<< ADDED category
    pub sheet_name: String,
    pub column_index: usize,
    pub new_validator: Option<ColumnValidator>, // Use Option to allow clearing/resetting
}