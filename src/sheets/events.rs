// src/sheets/events.rs
use bevy::prelude::Event;
use std::path::PathBuf;
use std::collections::HashSet;

use super::definitions::{ColumnDefinition, ColumnValidator, SheetGridData};

// --- Existing Events ---
#[derive(Event, Debug, Clone)]
pub struct AddSheetRowRequest {
    pub category: Option<String>,
    pub sheet_name: String,
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
}

/// Event fired when sheet data (grid or metadata affecting structure/validation)
/// is directly modified in the SheetRegistry.
/// The new `handle_sheet_render_cache_update` system listens to this.
#[derive(Event, Debug, Clone)]
pub struct SheetDataModifiedInRegistryEvent {
    pub category: Option<String>,
    pub sheet_name: String,
    // Optional: Could add a field here like `reason: ModificationReason`
    // (e.g., CellEdit, RowAdded, ColumnTypeChanged) to allow more granular
    // updates in the render cache system, but for now, just sheet identifier.
}

/// Event to explicitly request a revalidation and render cache rebuild for a sheet.
/// The new `handle_sheet_render_cache_update` system listens to this.
#[derive(Event, Debug, Clone)]
pub struct RequestSheetRevalidation {
    pub category: Option<String>,
    pub sheet_name: String,
}