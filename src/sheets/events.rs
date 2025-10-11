// src/sheets/events.rs
use bevy::prelude::Event;
use std::collections::HashSet;
use std::path::PathBuf;

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
    // Optional: initial cell values to set on the newly inserted row (at insert time)
    // Vector of (col_index, value)
    pub initial_values: Option<Vec<(usize, String)>>,
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

// Move a sheet between categories (or to/from root)
#[derive(Event, Debug, Clone)]
pub struct RequestMoveSheetToCategory {
    pub from_category: Option<String>,
    pub sheet_name: String,
    pub to_category: Option<String>,
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
    // NEW: Source column indices to snapshot into structure cells when converting to Structure
    pub structure_source_columns: Option<Vec<usize>>,
    // NEW: Optional key parent column index captured at initial structure creation
    pub key_parent_column_index: Option<usize>,
    // NEW: Original validator of the target column BEFORE switching to Structure (for self-inclusion case)
    pub original_self_validator: Option<ColumnValidator>,
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
pub struct AiTaskResult {
    pub original_row_index: usize,
    pub result: Result<Vec<String>, String>,
    pub raw_response: Option<String>,
    // Mapping snapshot for this row at send time (non-structure columns actually included)
    pub included_non_structure_columns: Vec<usize>,
    // Number of context-only prefix columns included ahead of non-structure columns
    pub context_only_prefix_count: usize,
}

#[derive(Event, Debug, Clone)]
pub struct AiBatchTaskResult {
    pub original_row_indices: Vec<usize>,         // order sent
    pub result: Result<Vec<Vec<String>>, String>, // first N correspond to originals, extra rows are additions
    pub raw_response: Option<String>,
    pub included_non_structure_columns: Vec<usize>,
    pub key_prefix_count: usize, // number of leading key/context columns prefixed to each row in result
    // NEW: Indicates this batch was initiated from a prompt with zero selected rows
    pub prompt_only: bool,
    pub kind: AiBatchResultKind,
}

#[derive(Debug, Clone)]
pub enum AiBatchResultKind {
    Root {
        // Optional structure context for nested processing
        structure_context: Option<StructureProcessingContext>,
    },
    /// Phase 2 deep review call - all rows treated as existing, automatic after Phase 1
    DeepReview {
        /// Indices of rows that are duplicates (marked for merge UI)
        duplicate_indices: Vec<usize>,
        /// Number of original + duplicate rows (remaining are AI-added with minimal data)
        established_row_count: usize,
    },
}

#[derive(Debug, Clone)]
pub struct StructureProcessingContext {
    pub root_category: Option<String>,
    pub root_sheet: String,
    pub structure_path: Vec<usize>,
    pub target_rows: Vec<usize>,
    /// Original structure row counts per parent (before AI adds rows)
    pub original_row_partitions: Vec<usize>,
    /// Updated structure row counts per parent (includes AI-added rows)
    pub row_partitions: Vec<usize>,
    pub generation_id: u64,
}

#[derive(Event, Debug, Clone)]
pub struct SheetDataModifiedInRegistryEvent {
    pub category: Option<String>,
    pub sheet_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestToggleAiRowGeneration {
    pub category: Option<String>,
    pub sheet_name: String,
    pub enabled: bool,
    /// When Some, identifies the nested structure path starting at the root sheet column index.
    /// Empty path (None) targets the root sheet itself. Nested indices drill into structure schemas.
    pub structure_path: Option<Vec<usize>>,
    /// For structure toggles, Some(value) applies an override, None reverts to general/default behavior.
    pub structure_override: Option<bool>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestUpdateAiSendSchema {
    pub category: Option<String>,
    pub sheet_name: String,
    /// None targets the root sheet itself; Some(path) identifies nested structure path.
    pub structure_path: Option<Vec<usize>>,
    /// Indices (within the targeted schema) of non-structure columns that should remain included.
    pub included_columns: Vec<usize>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestUpdateColumnAiInclude {
    pub category: Option<String>,
    pub sheet_name: String,
    pub column_index: usize,
    pub include: bool,
}

#[derive(Event, Debug, Clone)]
pub struct RequestBatchUpdateColumnAiInclude {
    pub category: Option<String>,
    pub sheet_name: String,
    pub updates: Vec<(usize, bool)>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestUpdateAiStructureSend {
    pub category: Option<String>,
    pub sheet_name: String,
    /// Path identifying the structure node to toggle (first element is root column index).
    pub structure_path: Vec<usize>,
    pub include: bool,
}

#[derive(Event, Debug, Clone)]
pub struct RequestCreateAiSchemaGroup {
    pub category: Option<String>,
    pub sheet_name: String,
    pub desired_name: Option<String>,
}

#[derive(Event, Debug, Clone)]
pub struct RequestRenameAiSchemaGroup {
    pub category: Option<String>,
    pub sheet_name: String,
    pub old_name: String,
    pub new_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteAiSchemaGroup {
    pub category: Option<String>,
    pub sheet_name: String,
    pub group_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestSelectAiSchemaGroup {
    pub category: Option<String>,
    pub sheet_name: String,
    pub group_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestSheetRevalidation {
    pub category: Option<String>,
    pub sheet_name: String,
}

// --- NEW: Events for structure navigation ---
#[derive(Event, Debug, Clone)]
pub struct OpenStructureViewEvent {
    pub parent_category: Option<String>,
    pub parent_sheet: String,
    pub row_index: usize,
    pub col_index: usize,
}

#[derive(Event, Debug, Clone)]
pub struct CloseStructureViewEvent;

// --- Category (Folder) management events ---
#[derive(Event, Debug, Clone)]
pub struct RequestCreateCategory {
    pub name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestDeleteCategory {
    pub name: String,
}

// --- IO: Create directory for category on disk ---
#[derive(Event, Debug, Clone)]
pub struct RequestCreateCategoryDirectory {
    pub name: String,
}

// --- Category rename events ---
#[derive(Event, Debug, Clone)]
pub struct RequestRenameCategory {
    pub old_name: String,
    pub new_name: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestRenameCategoryDirectory {
    pub old_name: String,
    pub new_name: String,
}

// --- Clipboard events ---
#[derive(Event, Debug, Clone)]
pub struct RequestCopyCell {
    pub category: Option<String>,
    pub sheet_name: String,
    pub row_index: usize,
    pub col_index: usize,
}

#[derive(Event, Debug, Clone)]
pub struct RequestPasteCell {
    pub category: Option<String>,
    pub sheet_name: String,
    pub row_index: usize,
    pub col_index: usize,
}

// --- Database Migration events ---
#[derive(Event, Debug, Clone)]
pub struct RequestMigrateJsonToDb {
    pub json_folder_path: PathBuf,
    pub target_db_path: PathBuf,
    pub create_new_db: bool,
}

/// Request to upload a single JSON file and migrate it into the current database as a table
#[derive(Event, Debug, Clone)]
pub struct RequestUploadJsonToCurrentDb {
    pub target_db_name: String, // The database (category) to add the table to
}

#[derive(Event, Debug, Clone)]
pub struct MigrationCompleted {
    pub success: bool,
    pub report: String,
}

#[derive(Event, Debug, Clone)]
pub struct MigrationProgress {
    pub total: usize,
    pub completed: usize,
    pub current_sheet: Option<String>,
    pub message: String,
}

#[derive(Event, Debug, Clone)]
pub struct RequestExportSheetToJson {
    pub db_path: PathBuf,
    pub table_name: String,
    pub output_folder: PathBuf,
}
