// Navigation-related type definitions for editor state

use super::review_types::{RowReview, NewRowReview};

/// Navigation context for real structure sheets (not virtual)
/// When navigating into a structure column, we open the real structure sheet
/// with a hidden filter that shows only rows related to the parent row
#[derive(Debug, Clone)]
pub struct StructureNavigationContext {
    /// The real structure sheet name (e.g., "TacticalFrontlines_Items")
    pub structure_sheet_name: String,
    /// Parent sheet information
    pub parent_category: Option<String>,
    pub parent_sheet_name: String,
    /// Parent row's primary key value (for filtering on parent_key column)
    pub parent_row_key: String,
    /// Ancestor display values for UI breadcrumb display
    /// Order: [deepest_ancestor_display, ..., immediate_parent_display]
    /// Example: ["Game Name", "Platform Name"] for Games -> Platforms -> Store navigation
    pub ancestor_keys: Vec<String>,
    /// Ancestor row_index values for AI parent chain filtering
    /// Order matches ancestor_keys: [deepest_ancestor_row_index, ..., immediate_parent_row_index]
    /// Example: ["3770", "1234"] - numeric strings that can be parsed as usize
    pub ancestor_row_indices: Vec<String>,
}

/// Navigation context for AI review breadcrumb trail
#[derive(Debug, Clone)]
pub struct NavigationContext {
    /// Sheet name at this navigation level
    pub sheet_name: String,
    /// Category at this navigation level
    pub category: Option<String>,
    /// Parent row index (for display in breadcrumb)
    #[allow(dead_code)]
    pub parent_row_index: Option<usize>,
    /// Parent review index (position in ai_row_reviews or ai_new_row_reviews)
    /// This is what StructureReviewEntry uses for parent_row_index matching
    pub parent_review_index: Option<usize>,
    /// Parent row display name (cached for breadcrumb)
    pub parent_display_name: Option<String>,
    /// Cached review state at this level (to restore when navigating back)
    pub cached_row_reviews: Vec<RowReview>,
    /// Cached new row reviews at this level
    pub cached_new_row_reviews: Vec<NewRowReview>,
}

/// Filter context for child table reviews
#[derive(Debug, Clone)]
pub struct ParentFilter {
    /// Filter child rows where parent_key equals this row_index
    pub parent_row_index: usize,
}

/// Status indicator for structure column drill-down buttons
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum StructureStatus {
    /// No review data available for this structure
    NotReviewed,
    /// Review data available but not yet approved
    Pending { row_count: usize },
    /// Review data has been approved
    Approved,
}
