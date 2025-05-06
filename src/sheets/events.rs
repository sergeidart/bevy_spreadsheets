// src/sheets/events.rs
use bevy::prelude::Event;
use std::path::PathBuf;
use std::collections::HashSet; // <-- Add this import

// Use definitions (including new ColumnDefinition)
use super::definitions::{ColumnDefinition, ColumnValidator, SheetGridData}; // Keep SheetGridData if needed

#[derive(Event, Debug, Clone)]
pub struct AddSheetRowRequest {
    pub category: Option<String>,
    pub sheet_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct JsonSheetUploaded {
    pub category: Option<String>, // e.g., None for direct uploads to root
    pub desired_sheet_name: String,
    pub original_filename: String, // Just the filename part
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
    pub relative_path: PathBuf, // Relative to data_sheets base dir
}

#[derive(Event, Debug, Clone)]
pub struct RequestRenameSheetFile {
    pub old_relative_path: PathBuf, // Relative to data_sheets base dir
    pub new_relative_path: PathBuf, // Relative to data_sheets base dir
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
    pub row_index: usize, // Use original row index
    pub col_index: usize,
    pub new_value: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestUpdateColumnValidator {
    pub category: Option<String>,
    pub sheet_name: String,
    pub column_index: usize,
    pub new_validator: Option<ColumnValidator>, // Use Option to allow clearing
}

// --- ADDED: Event to request deleting specific rows ---
#[derive(Event, Debug, Clone)]
pub struct RequestDeleteRows {
    pub category: Option<String>,
    pub sheet_name: String,
    // Use HashSet for potentially unordered indices from UI selection
    pub row_indices: HashSet<usize>,
}

// --- ADDED: Event to request updating column width ---
#[derive(Event, Debug, Clone)]
pub struct RequestUpdateColumnWidth {
    pub category: Option<String>,
    pub sheet_name: String,
    pub column_index: usize,
    pub new_width: f32,
}
// --- END ADD ---

// --- ADDED: AI Task Result Event ---
#[derive(Event, Debug, Clone)]
pub struct AiTaskResult {
    pub original_row_index: usize,
    // Ok(suggestion) or Err(message)
    pub result: Result<Vec<String>, String>,
}

// --- ADDED: Event to signal data modification in registry for cache invalidation ---
#[derive(Event, Debug, Clone)]
pub struct SheetDataModifiedInRegistryEvent {
    pub category: Option<String>,
    pub sheet_name: String,
}