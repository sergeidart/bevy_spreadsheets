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

/// Batch processing context - stored when processing AI results
/// Fields are retained for debugging and future use even if not currently accessed
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BatchProcessingContext {
    /// Indices of rows that are duplicates (potential merges)
    pub duplicate_indices: Vec<usize>,
    /// Number of original rows sent
    pub original_count: usize,
    /// Included column indices
    pub included_columns: Vec<usize>,
    /// Category context
    pub category: Option<String>,
    /// Sheet name context
    pub sheet_name: String,
    /// Original row indices (for mapping)
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
    /// Orphaned rows that belong to this child table but have unmatched parent prefix.
    /// These are duplicated across all parent entries so they're always visible.
    /// Each inner Vec is the row data (column values).
    pub orphaned_ai_rows: Vec<Vec<String>>,
    /// The claimed ancestry for each orphaned row (what the AI said the parent was).
    /// Parallel array to orphaned_ai_rows.
    pub orphaned_claimed_ancestries: Vec<Vec<String>>,
    /// Track which orphaned rows have been decided (accepted/declined).
    /// Parallel array to orphaned_ai_rows. True = decided, False = pending.
    pub orphaned_decided: Vec<bool>,
}

impl StructureReviewEntry {
    pub fn is_undecided(&self) -> bool {
        self.has_changes && !self.decided
    }
}
