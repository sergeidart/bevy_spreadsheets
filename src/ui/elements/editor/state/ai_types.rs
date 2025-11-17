// AI-related type definitions for editor state

use super::review_types::{RowReview, NewRowReview};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiModeState {
    #[default]
    Idle,
    Preparing,
    Submitting,
    ResultsReady,
    Reviewing,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ThrottledAiAction {
    UpdateCell {
        row_index: usize,
        col_index: usize,
        value: String,
    },
    AddRow {
        initial_values: Vec<(usize, String)>,
    },
}

/// Log entry for a single AI call (newest entries are added to the front)
#[derive(Debug, Clone)]
pub struct AiCallLogEntry {
    /// Human-readable status (e.g., "2/4 received", "Completed", "Error")
    pub status: String,
    /// The full response JSON (if available)
    pub response: Option<String>,
    /// The full request payload that was sent
    pub request: Option<String>,
    /// Whether this is an error entry
    pub is_error: bool,
}

/// Phase 1 intermediate data - stored after initial discovery call, before Phase 2 deep review
#[derive(Debug, Clone)]
pub struct Phase1IntermediateData {
    /// All rows from Phase 1 AI response
    pub all_ai_rows: Vec<Vec<String>>,
    /// Indices of rows that are duplicates (in all_ai_rows, after originals)
    pub duplicate_indices: Vec<usize>,
    /// Number of original rows sent
    pub original_count: usize,
    /// Included column indices
    pub included_columns: Vec<usize>,
    /// Context for sending Phase 2
    pub category: Option<String>,
    pub sheet_name: String,
    #[allow(dead_code)] // Used in other modules but compiler doesn't track cross-module usage
    pub key_prefix_count: usize,
    /// Original row indices from Phase 1 (for structure processing)
    pub original_row_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct StructureParentContext {
    pub parent_category: Option<String>,
    pub parent_sheet: String,
    pub parent_row: usize,
    pub parent_col: usize,
}

#[derive(Debug, Clone)]
pub struct StructureSendJob {
    pub root_category: Option<String>,
    pub root_sheet: String,
    /// Identifies the nested structure (first element is root column index)
    pub structure_path: Vec<usize>,
    /// Optional friendly label resolved from metadata for status logs
    pub label: Option<String>,
    /// Snapshot of row indices (within the root sheet) that triggered this job
    pub target_rows: Vec<usize>,
    pub generation_id: u64,
}

#[derive(Debug, Clone)]
pub struct StructureNewRowContext {
    pub new_row_index: usize,
    pub non_structure_values: Vec<(usize, String)>,
    /// Full AI row from Phase 1, including structure columns (stored as JSON strings)
    pub full_ai_row: Option<Vec<String>>,
}

/// Context for navigating into structure detail view during AI review
#[derive(Debug, Clone)]
pub struct StructureDetailContext {
    /// Root category of the sheet containing the structure
    pub root_category: Option<String>,
    /// Root sheet name containing the structure
    pub root_sheet: String,
    /// Index of parent row in ai_row_reviews (for existing rows)
    pub parent_row_index: Option<usize>,
    /// Index of parent row in ai_new_row_reviews (for new rows)
    pub parent_new_row_index: Option<usize>,
    /// Structure path to the structure being viewed
    pub structure_path: Vec<usize>,
    /// Legacy fields kept for compatibility but no longer used
    #[allow(dead_code)]
    pub hydrated: bool,
    pub saved_row_reviews: Vec<RowReview>,
    pub saved_new_row_reviews: Vec<NewRowReview>,
}

#[derive(Debug, Clone)]
pub struct StructureReviewEntry {
    pub root_category: Option<String>,
    pub root_sheet: String,
    pub parent_row_index: usize,
    pub parent_new_row_index: Option<usize>,
    /// Path from root sheet to this structure (first element is column index in root, subsequent are structure col indices)
    pub structure_path: Vec<usize>,
    pub has_changes: bool,
    pub accepted: bool,
    pub rejected: bool,
    pub decided: bool,
    /// The original structure rows parsed from the cell
    pub original_rows: Vec<Vec<String>>,
    /// The AI-suggested structure rows
    pub ai_rows: Vec<Vec<String>>,
    /// The merged rows (combines accepted changes)
    pub merged_rows: Vec<Vec<String>>,
    /// Per-row, per-column difference flags
    pub differences: Vec<Vec<bool>>,
    pub schema_headers: Vec<String>,
    #[allow(dead_code)]
    pub original_rows_count: usize,
}

impl StructureReviewEntry {
    pub fn is_undecided(&self) -> bool {
        self.has_changes && !self.decided
    }
}
