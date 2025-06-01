// src/sheets/events.rs
use bevy::prelude::Event;
use std::path::PathBuf;
use std::collections::HashSet;

use super::definitions::ColumnValidator; 

// NEW: Event for creating a new sheet
#[derive(Event, Debug, Clone)]
pub struct RequestCreateNewSheet {
    pub desired_name: String,
    pub category: Option<String>, // None for root category
}

#[derive(Event, Debug, Clone)]
pub struct AddSheetRowRequest {
    pub category: Option<String>,
    pub sheet_name: String,
}
// ... (rest of the existing events remain the same) ...
#[derive(Event, Debug, Clone)]
pub struct RequestAddColumn {
    pub category: Option<String>,
    pub sheet_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestReorderColumn {
    pub category: Option<String>,
    pub sheet_name: String,
    pub old_index: usize,
    pub new_index: usize,
}

#[derive(Event, Debug, Clone)]
pub struct JsonSheetUploaded {
    pub category: Option<String>,
    pub desired_sheet_name: String,
    pub original_filename: String,
    pub grid_data: Vec<Vec<String>>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestRenameSheet {
    pub category: Option<String>,
    pub old_name: String,
    pub new_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteSheet {
    pub category: Option<String>,
    pub sheet_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteSheetFile {
    pub relative_path: PathBuf,
}

#[derive(Event, Debug, Clone)]
pub struct RequestRenameSheetFile {
    pub old_relative_path: PathBuf,
    pub new_relative_path: PathBuf,
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
    pub category: Option<String>,
    pub sheet_name: String,
    pub column_index: usize,
    pub new_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct UpdateCellEvent {
    pub category: Option<String>,
    pub sheet_name: String,
    pub row_index: usize,
    pub col_index: usize,
    pub new_value: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestUpdateColumnValidator {
    pub category: Option<String>,
    pub sheet_name: String,
    pub column_index: usize,
    pub new_validator: Option<ColumnValidator>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteRows {
    pub category: Option<String>,
    pub sheet_name: String,
    pub row_indices: HashSet<usize>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteColumns {
    pub category: Option<String>,
    pub sheet_name: String,
    pub column_indices: HashSet<usize>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestUpdateColumnWidth {
    pub category: Option<String>,
    pub sheet_name: String,
    pub column_index: usize,
    pub new_width: f32,
}

#[derive(Event, Debug, Clone)]
pub struct AiTaskResult {
    pub original_row_index: usize,
    pub result: Result<Vec<String>, String>, 
    pub raw_response: Option<String>,      
}

#[derive(Event, Debug, Clone)]
pub struct SheetDataModifiedInRegistryEvent {
    pub category: Option<String>,
    pub sheet_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestSheetRevalidation {
    pub category: Option<String>,
    pub sheet_name: String,
}