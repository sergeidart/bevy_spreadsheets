// Review-related type definitions for editor state

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewChoice {
    Original,
    AI,
}

#[derive(Debug, Clone)]
pub struct RowReview {
    pub row_index: usize,
    pub original: Vec<String>,
    pub ai: Vec<String>,
    pub choices: Vec<ReviewChoice>, // length == editable (non-structure) columns shown order
    pub non_structure_columns: Vec<usize>, // mapping for editable subset
    /// Track which key columns (by position) have override enabled (default false)
    pub key_overrides: std::collections::HashMap<usize, bool>,
    /// Editable ancestor key values (indexed by ancestor key position)
    pub ancestor_key_values: Vec<String>,
    /// Cached dropdown options for each ancestor level
    /// Key: ancestor index, Value: (cached_ancestors_snapshot, options)
    pub ancestor_dropdown_cache: HashMap<usize, (Vec<String>, Vec<String>)>,
}

#[derive(Debug, Clone)]
pub struct NewRowReview {
    pub ai: Vec<String>,
    pub non_structure_columns: Vec<usize>,
    pub duplicate_match_row: Option<usize>,
    pub choices: Option<Vec<ReviewChoice>>,
    pub merge_selected: bool,
    pub merge_decided: bool,
    // Original row data for the matched duplicate row (used for merge comparison)
    pub original_for_merge: Option<Vec<String>>,
    /// Track which key columns (by position) have override enabled (default false)
    pub key_overrides: std::collections::HashMap<usize, bool>,
    /// Editable ancestor key values (indexed by ancestor key position)
    pub ancestor_key_values: Vec<String>,
    /// Cached dropdown options for each ancestor level
    /// Key: ancestor index, Value: (cached_ancestors_snapshot, options)
    pub ancestor_dropdown_cache: HashMap<usize, (Vec<String>, Vec<String>)>,
}
